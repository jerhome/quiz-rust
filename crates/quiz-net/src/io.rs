use crate::protocol::MAX_LINE_BYTES;
use serde::Serialize;
use std::io::{Error, ErrorKind};
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc::UnboundedReceiver;

/// The cap is enforced while buffering, so a peer cannot make us allocate without bound.
pub(crate) async fn read_line_limited<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> std::io::Result<Option<String>> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(None);
        }
        let (take, found) = match available.iter().position(|b| *b == b'\n') {
            Some(i) => (i, true),
            None => (available.len(), false),
        };
        buf.extend_from_slice(&available[..take]);
        reader.consume(take + usize::from(found));
        if buf.len() > MAX_LINE_BYTES {
            return Err(Error::new(ErrorKind::InvalidData, "ligne trop longue"));
        }
        if found {
            return String::from_utf8(buf)
                .map(Some)
                .map_err(|_| Error::new(ErrorKind::InvalidData, "UTF-8 invalide"));
        }
    }
}

pub(crate) async fn write_line<W: AsyncWrite + Unpin, T: Serialize>(
    w: &mut W,
    msg: &T,
) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(msg).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    bytes.push(b'\n');
    w.write_all(&bytes).await?;
    w.flush().await
}

/// Shuts the write half down when the channel closes, so the peer sees EOF.
/// Extra liveness traffic written by `writer_loop` next to the regular messages.
pub(crate) enum Beat {
    /// Host side: a ping every `interval`.
    Every(tokio::time::Interval, &'static [u8]),
    /// Client side: a pong each time the reader signals a ping.
    OnSignal(UnboundedReceiver<()>, &'static [u8]),
}

impl Beat {
    pub(crate) fn every(interval: Duration, line: &'static [u8]) -> Self {
        let start = tokio::time::Instant::now() + interval;
        Beat::Every(tokio::time::interval_at(start, interval), line)
    }

    async fn next(&mut self) -> &'static [u8] {
        match self {
            Beat::Every(iv, line) => {
                iv.tick().await;
                line
            }
            Beat::OnSignal(rx, line) => match rx.recv().await {
                Some(()) => line,
                None => std::future::pending().await,
            },
        }
    }
}

/// Shuts the write half down when the channel closes, so the peer sees EOF.
pub(crate) async fn writer_loop<W: AsyncWrite + Unpin, T: Serialize>(
    mut w: W,
    mut rx: UnboundedReceiver<T>,
    mut beat: Beat,
) {
    loop {
        tokio::select! {
            m = rx.recv() => match m {
                Some(m) => {
                    if write_line(&mut w, &m).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            line = beat.next() => {
                if w.write_all(line).await.is_err() || w.flush().await.is_err() {
                    break;
                }
            }
        }
    }
    let _ = w.shutdown().await;
}

/// TCP keepalive (10 s idle, 5 s probes, 3 retries where the OS lets us choose).
/// Windows ignores the retry count, hence the application heartbeat as well.
pub(crate) fn enable_keepalive(stream: &tokio::net::TcpStream) {
    let ka = socket2::TcpKeepalive::new().with_time(Duration::from_secs(10));
    #[cfg(any(target_os = "linux", windows))]
    let ka = ka.with_interval(Duration::from_secs(5));
    #[cfg(target_os = "linux")]
    let ka = ka.with_retries(3);
    if let Err(e) = socket2::SockRef::from(stream).set_tcp_keepalive(&ka) {
        tracing::debug!("TCP keepalive unavailable: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn reads_lines_and_eof() {
        let mut r = BufReader::new(&b"a\nbc\n"[..]);
        assert_eq!(
            read_line_limited(&mut r).await.unwrap().as_deref(),
            Some("a")
        );
        assert_eq!(
            read_line_limited(&mut r).await.unwrap().as_deref(),
            Some("bc")
        );
        assert_eq!(read_line_limited(&mut r).await.unwrap(), None);
    }

    #[tokio::test]
    async fn rejects_too_long_line() {
        let data = vec![b'x'; MAX_LINE_BYTES + 10];
        let mut r = BufReader::new(&data[..]);
        assert!(read_line_limited(&mut r).await.is_err());
    }

    #[tokio::test]
    async fn rejects_invalid_utf8() {
        let mut r = BufReader::new(&[0xff, 0xfe, b'\n'][..]);
        assert!(read_line_limited(&mut r).await.is_err());
    }
}
