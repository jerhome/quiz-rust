use std::time::Duration;

/// Application-level liveness check, on top of TCP keepalive.
///
/// The host pings every `interval`; each side drops the connection after
/// `idle_timeout` without receiving anything. TCP keepalive alone is not enough:
/// Windows ignores the retry count, and NATs/VPNs may silently drop flows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartbeatConfig {
    pub interval: Duration,
    pub idle_timeout: Duration,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(15),
        }
    }
}
