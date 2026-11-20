//! Bounds apply before spawning a connection task, including the TLS handshake.
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(super) const DEADLINE: Duration = Duration::from_secs(8);
const TOTAL: usize = 64;
const SOURCE: usize = 8;

#[derive(Default)]
pub(super) struct Admission {
    pending: HashMap<IpAddr, usize>,
    failures: HashMap<IpAddr, Penalty>,
}

#[derive(Clone, Copy)]
struct Penalty {
    count: usize,
    last: Instant,
}

impl Penalty {
    fn record(previous: Option<Self>, now: Instant) -> Self {
        let count = previous
            .filter(|p| now.duration_since(p.last) < Duration::from_secs(60))
            .map_or(1, |p| p.count.saturating_add(1));
        Self { count, last: now }
    }

    fn blocked(self, threshold: usize, now: Instant) -> bool {
        self.count >= threshold
            && now.duration_since(self.last)
                < Duration::from_secs(1 << (self.count - threshold).min(5))
    }
}

impl Admission {
    pub fn enter(state: &Arc<Mutex<Self>>, ip: IpAddr, now: Instant) -> Option<Permit> {
        let mut gate = state.lock().unwrap();
        let pending = gate.pending.values().sum::<usize>();
        // The last eight slots are for addresses without an in-flight sign-in.
        if pending >= TOTAL
            || (pending >= TOTAL - SOURCE && gate.pending.contains_key(&ip))
            || gate.pending.get(&ip).copied().unwrap_or_default() >= SOURCE
            || gate.failures.get(&ip).is_some_and(|p| p.blocked(3, now))
        {
            return None;
        }
        *gate.pending.entry(ip).or_default() += 1;
        Some(Permit {
            state: state.clone(),
            ip,
        })
    }
}
pub(super) struct Permit {
    state: Arc<Mutex<Admission>>,
    ip: IpAddr,
}
impl Permit {
    pub fn failed(&self) {
        let mut gate = self.state.lock().unwrap();
        let now = Instant::now();
        gate.failures
            .retain(|_, p| now.duration_since(p.last) < Duration::from_secs(60));
        let penalty = Penalty::record(gate.failures.get(&self.ip).copied(), now);
        if gate.failures.len() < 1024 || gate.failures.contains_key(&self.ip) {
            gate.failures.insert(self.ip, penalty);
        }
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut gate = self.state.lock().unwrap();
        let n = gate.pending.get_mut(&self.ip).unwrap();
        *n -= 1;
        if *n == 0 {
            gate.pending.remove(&self.ip);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_returns_on_every_exit() {
        let gate = Default::default();
        let now = Instant::now();
        let mut held = vec![];
        for i in 1..=7 {
            let ip = [127, 0, 0, i].into();
            for _ in 0..SOURCE {
                held.push(Admission::enter(&gate, ip, now).unwrap());
            }
            assert!(Admission::enter(&gate, ip, now).is_none());
        }
        for i in 8..15 {
            let ip = [127, 0, 0, i].into();
            held.push(Admission::enter(&gate, ip, now).unwrap());
            assert!(Admission::enter(&gate, ip, now).is_none());
        }
        let owner = [127, 0, 0, 15].into();
        held.push(Admission::enter(&gate, owner, now).unwrap());
        assert!(Admission::enter(&gate, [127, 0, 0, 16].into(), now).is_none());
        drop(held);
        assert!(Admission::enter(&gate, [127, 0, 0, 1].into(), now).is_some());
    }
    #[tokio::test]
    async fn owner_can_prove_the_key_through_the_last_reserved_slot() {
        use crate::{HostConfig, HostState};
        let gate = Arc::new(Mutex::new(Admission::default()));
        let now = Instant::now();
        {
            let mut state = gate.lock().unwrap();
            for n in 0..1024u32 {
                state.failures.insert(
                    std::net::Ipv4Addr::from(0xc0000000 + n).into(),
                    Penalty {
                        count: usize::MAX,
                        last: now,
                    },
                );
            }
            for n in 1..=7 {
                state.pending.insert([192, 0, 2, n].into(), SOURCE);
            }
            for n in 8..15 {
                state.pending.insert([192, 0, 2, n].into(), 1);
            }
        }
        let server = sunna_transport::Server::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = server.local_addr().unwrap();
        let identity = Some(sunna_transport::auth::certificate_pin(&server.cert).unwrap());
        let key = "aa".repeat(16);
        let host = HostState {
            config: HostConfig {
                token: key.clone(),
                require_proof: true,
                name: "test".into(),
                width: 8,
                height: 8,
                fps: 1,
                codec: "raw".into(),
                max_bitrate_bps: 1000,
                about: Default::default(),
                clipboard: false,
                audio: false,
                fast_lane: false,
                simulate_loss: 0.,
                display_follows_viewer: false,
            },
            desktop: tokio::sync::watch::channel(Ok((8, 8))).1,
            local_desktop: true,
            devices: None,
            new_source: Box::new(|_| unreachable!()),
            new_encoder: Box::new(|_| unreachable!()),
            new_injector: Box::new(|| unreachable!()),
            busy: Default::default(),
            viewer: Default::default(),
        };
        let task = tokio::spawn(crate::accept_sessions(server, Arc::new(host), gate.clone()));
        let trust = sunna_client::auth::Trust {
            identity,
            ..Default::default()
        };
        let result = sunna_client::probe_with(addr, "sunna", &key, DEADLINE, &trust)
            .await
            .unwrap();
        assert!(result.token_ok);
        assert_eq!(
            gate.lock().unwrap().pending.values().sum::<usize>(),
            TOTAL - 1
        );
        task.abort();
    }

    #[test]
    fn wrong_keys_slow_down_then_expire() {
        let gate = Default::default();
        let ip = [127, 0, 0, 1].into();
        for _ in 0..3 {
            Admission::enter(&gate, ip, Instant::now())
                .unwrap()
                .failed();
        }
        assert!(Admission::enter(&gate, ip, Instant::now()).is_none());
        assert!(Admission::enter(&gate, ip, Instant::now() + Duration::from_secs(2)).is_some());
    }

    #[test]
    fn independent_failures_leave_capacity_for_a_new_source() {
        let gate = Default::default();
        for n in 1..=48 {
            let ip = [192, 0, 2, n].into();
            Admission::enter(&gate, ip, Instant::now())
                .unwrap()
                .failed();
        }
        let fresh = [192, 0, 2, 49].into();
        assert!(Admission::enter(&gate, fresh, Instant::now()).is_some());
        assert!(Admission::enter(&gate, fresh, Instant::now() + Duration::from_secs(2)).is_some());
    }
}
