use quiz_net::{discover_on, Host, HostConfig};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;

const T: Duration = Duration::from_secs(4);

fn free_udp_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test]
async fn every_announcement_is_reported_again() {
    let port = free_udp_port();
    let mut d = discover_on(port, None).await.unwrap();
    let u = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let pkt = br#"{"game":"quiz-rust","name":"Salon","port":7878}"#;
    for _ in 0..2 {
        u.send_to(pkt, ("127.0.0.1", port)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    for _ in 0..2 {
        let h = timeout(T, d.hosts.recv()).await.unwrap().unwrap();
        assert_eq!(h.name, "Salon");
    }
}

#[tokio::test]
async fn restarted_host_with_same_ip_port_and_name_is_signalled_again() {
    let dport = free_udp_port();
    let tcp_port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut d = discover_on(dport, None).await.unwrap();
    let cfg = HostConfig {
        name: "Salon".into(),
        port: tcp_port,
        announce: true,
        discovery_port: dport,
        announce_ip: "127.0.0.1".parse().unwrap(),
        ..HostConfig::default()
    };
    let first = Host::start(cfg.clone()).await.unwrap();
    let a = timeout(T, d.hosts.recv()).await.unwrap().unwrap();
    first.shutdown();
    drop(first);
    tokio::time::sleep(Duration::from_millis(300)).await;
    while d.hosts.try_recv().is_ok() {}
    let _second = Host::start(cfg).await.unwrap();
    let b = timeout(T, d.hosts.recv())
        .await
        .expect("restarted host not re-signalled")
        .unwrap();
    assert_eq!((a.name, a.addr.port()), (b.name, b.addr.port()));
}
