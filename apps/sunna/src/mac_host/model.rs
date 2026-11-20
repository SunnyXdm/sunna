//! Pure decisions shared by the window and the menu bar process.
use std::net::{Ipv4Addr, SocketAddr};

pub const LABEL: &str = "dev.sunna.host";
pub const WAITING: &str = "Waiting for permission on that Mac";

pub fn plist(executable: &str, log: &str) -> String {
    fn escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>Label</key><string>{LABEL}</string>
<key>AssociatedBundleIdentifiers</key><array><string>dev.sunna.app</string></array>
<key>ProgramArguments</key><array><string>{}</string><string>host</string><string>--agent</string></array>
<key>LimitLoadToSessionType</key><string>Aqua</string>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
<key>StandardErrorPath</key><string>{}</string>
</dict></plist>
"#,
        escape(executable),
        escape(log)
    )
}

pub fn listener(
    explicit: Option<SocketAddr>,
    tail: Option<Ipv4Addr>,
    lan: Option<Ipv4Addr>,
) -> Option<(SocketAddr, &'static str)> {
    let address = explicit.or_else(|| Some(SocketAddr::new(tail.or(lan)?.into(), 48800)))?;
    let reach = if tail.is_some_and(|ip| std::net::IpAddr::V4(ip) == address.ip()) {
        "tailscale"
    } else {
        "lan"
    };
    Some((address, reach))
}

pub fn listen_setting(
    setting: Option<&str>,
    tail: Option<Ipv4Addr>,
    lan: Option<Ipv4Addr>,
) -> Result<(SocketAddr, &'static str), &'static str> {
    let choice = match setting.unwrap_or("auto") {
        "auto" => listener(None, tail, lan),
        "tailscale" => listener(None, tail, None),
        "lan" => listener(None, None, lan),
        other => {
            let addr = other
                .parse::<SocketAddr>()
                .or_else(|_| other.parse().map(|ip| SocketAddr::new(ip, 48800)))
                .map_err(|_| {
                    "Choose auto, tailscale, lan, or a local IP address with an optional port."
                })?;
            if addr.ip().is_unspecified() {
                return Err("Choose a specific interface address for this Mac.");
            }
            listener(Some(addr), tail, lan)
        }
    };
    choice.ok_or("Connect this Mac to the selected network and try again.")
}

pub fn state(
    running: bool,
    enabled: bool,
    screen: bool,
    input: bool,
    reason: Option<&str>,
) -> &'static str {
    match (running, enabled, screen && input) {
        (true, _, false) => "needs-permission",
        (true, _, true) if reason.is_some() => "unavailable",
        (true, _, true) => "on",
        (false, true, _) => "starting",
        (false, false, _) => "off",
    }
}

pub fn menu(
    reason: Option<&str>,
    reach: &str,
    address: SocketAddr,
    viewer: Option<&str>,
) -> [String; 3] {
    let network = if reach == "tailscale" {
        "on your Tailscale network"
    } else {
        "on this local network"
    };
    [
        reason.map_or_else(
            || "● Sharing this Mac".into(),
            |r| format!("○ Not sharing — {r}"),
        ),
        format!("{network} · {address}"),
        viewer.map_or_else(
            || "Nobody is connected".into(),
            |who| format!("{who} is connected"),
        ),
    ]
}

#[derive(Clone, Copy)]
pub enum Action {
    Open,
    Copy,
    Stop,
    Quit,
}
impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open Sunna",
            Self::Copy => "Copy Link",
            Self::Stop => "Stop Sharing",
            Self::Quit => "Quit Sunna Host",
        }
    }
}
pub const ACTIONS: [Option<Action>; 6] = [
    None,
    Some(Action::Open),
    Some(Action::Copy),
    Some(Action::Stop),
    None,
    Some(Action::Quit),
];
pub fn confirmation(quit: bool, agent: bool, viewer: Option<&str>) -> Option<String> {
    let action = if quit && agent {
        "Sharing will stop until the next login or until you turn the switch on again."
    } else if quit {
        "Sharing will stop until you start the host again."
    } else {
        "Stop sharing anyway?"
    };
    match viewer {
        Some(who) => Some(format!("{who} is connected. {action}")),
        None if quit => Some(action.into()),
        None => None,
    }
}

#[derive(Debug, PartialEq)]
pub enum Restart {
    Keep,
    Wait,
    Replace,
}
pub fn restart(changed: bool, viewer: bool) -> Restart {
    match (changed, viewer) {
        (false, _) => Restart::Keep,
        (true, true) => Restart::Wait,
        (true, false) => Restart::Replace,
    }
}

/// Parse only the local listening endpoint of the historical run.sh host.
pub fn legacy_listener(output: &str) -> Result<Option<(i32, SocketAddr)>, &'static str> {
    let mut pid = None;
    let mut named = false;
    let mut found = None;
    for line in output.lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.parse::<i32>().ok().filter(|p| *p > 1);
            named = false;
        } else if line.starts_with('c') {
            named = line == "csunnad";
        } else if let Some(address) = line
            .strip_prefix('n')
            .and_then(|s| s.parse::<SocketAddr>().ok())
        {
            if let Some(p) = pid.filter(|_| named && address.port() == 48800) {
                if found.is_some() && found != Some((p, address)) {
                    return Err(
                        "Several Terminal hosts are running. Stop them in their Terminal windows.",
                    );
                }
                found = Some((p, address));
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_terminal_socket() {
        let a = "p12\ncsunnad\nn100.64.0.2:48800\n";
        assert_eq!(
            legacy_listener(a),
            Ok(Some((12, "100.64.0.2:48800".parse().unwrap())))
        );
        assert_eq!(
            legacy_listener("p1\ncsunnad\nn127.0.0.1:48800\np24\ncsunnad-dev\nn127.0.0.1:48800"),
            Ok(None)
        );
        assert!(legacy_listener(&format!("{a}p34\ncsunnad\nn127.0.0.1:48800")).is_err());
    }
    #[test]
    fn agent_golden() {
        assert_eq!(
            plist("/A & B/Sunna", "/Logs/<host>"),
            include_str!("agent.golden.plist")
        );
    }
    #[test]
    fn network() {
        let ts = "100.99.1.2".parse().unwrap();
        let wifi: Ipv4Addr = "192.168.1.12".parse().unwrap();
        let local = SocketAddr::new(wifi.into(), 48800);
        assert_eq!(
            listener(None, Some(ts), Some(wifi)),
            Some((SocketAddr::new(ts.into(), 48800), "tailscale"))
        );
        assert_eq!(listener(None, None, Some(wifi)), Some((local, "lan")));
        assert_eq!(listener(Some(local), Some(ts), None), Some((local, "lan")));
        assert_eq!(listener(None, None, None), None);
        assert!(listen_setting(Some("tailscale"), None, Some(wifi)).is_err());
        assert_eq!(
            listen_setting(Some("lan"), Some(ts), Some(wifi)),
            Ok((local, "lan"))
        );
        assert_eq!(
            listen_setting(Some("192.168.1.12"), None, None),
            Ok((local, "lan"))
        );
        assert!(listen_setting(Some("0.0.0.0:48800"), None, None).is_err());
    }
    #[test]
    fn sharing_states() {
        assert_eq!(state(false, false, false, false, None), "off");
        assert_eq!(state(false, true, true, true, None), "starting");
        for (screen, input) in [(false, false), (true, false), (false, true)] {
            assert_eq!(
                state(true, true, screen, input, Some(WAITING)),
                "needs-permission"
            );
        }
        assert_eq!(state(true, false, true, true, None), "on");
        assert_eq!(
            state(true, true, true, true, Some(FRAME_FAILED)),
            "unavailable"
        );
    }
    #[test]
    fn menu_commands_and_confirmation() {
        assert_eq!(
            ACTIONS.map(|a| a.map(Action::label)),
            [
                None,
                Some("Open Sunna"),
                Some("Copy Link"),
                Some("Stop Sharing"),
                None,
                Some("Quit Sunna Host")
            ]
        );
        assert_eq!(confirmation(false, true, None), None);
        assert_eq!(
            confirmation(false, true, Some("Priya")).as_deref(),
            Some("Priya is connected. Stop sharing anyway?")
        );
        assert!(confirmation(true, true, None)
            .unwrap()
            .contains("next login"));
        assert!(confirmation(true, false, Some("Alex"))
            .unwrap()
            .starts_with("Alex is connected."));
    }
    #[test]
    fn menu_states() {
        let addr = "192.168.1.2:48800".parse().unwrap();
        assert_eq!(
            menu(Some(WAITING), "lan", addr, None),
            [
                "○ Not sharing — Waiting for permission on that Mac",
                "on this local network · 192.168.1.2:48800",
                "Nobody is connected"
            ]
        );
        assert_eq!(
            menu(None, "tailscale", addr, Some("Priya")),
            [
                "● Sharing this Mac",
                "on your Tailscale network · 192.168.1.2:48800",
                "Priya is connected"
            ]
        );
    }
    #[test]
    fn restart_only_changed_idle_host() {
        assert_eq!(restart(false, true), Restart::Keep);
        assert_eq!(restart(false, false), Restart::Keep);
        assert_eq!(restart(true, true), Restart::Wait);
        assert_eq!(restart(true, false), Restart::Replace);
    }
}

/// A paused agent consumes the rewritten plist only at the next login.
pub fn paused_update(installed: &str, desired: &str, paused: bool) -> bool {
    paused && installed != desired
}

pub fn retry_tail(setting: Option<&str>, reach: &str, installed: bool) -> bool {
    installed && setting.unwrap_or("auto") == "auto" && reach == "lan"
}

pub struct FrameCheck {
    result: Option<bool>,
    retry: Option<std::time::Instant>,
    expires: Option<std::time::Instant>,
    generation: u64,
    checking: bool,
}

impl FrameCheck {
    pub const fn new() -> Self {
        Self {
            result: None,
            retry: None,
            expires: None,
            generation: 0,
            checking: false,
        }
    }

    pub fn begin(&mut self, allowed: bool, now: std::time::Instant) -> Option<u64> {
        if self.expires.is_some_and(|t| t <= now) {
            self.result = None;
            self.expires = None;
        }
        if !allowed {
            self.expires = None;
            self.result = None;
            self.retry = None;
            self.generation = self.generation.wrapping_add(1);
        } else if !self.checking && self.retry.is_none_or(|t| t <= now)
        {
            self.checking = true;
            return Some(self.generation);
        }
        None
    }

    pub fn complete(&mut self, generation: u64, success: bool, now: std::time::Instant) {
        self.checking = false;
        if generation == self.generation {
            self.result = Some(success);
            // Refresh while the last success is still fresh. A viewer checking
            // availability each second can keep streaming during the test.
            self.expires = success.then(|| now + std::time::Duration::from_secs(60));
            self.retry = Some(now + std::time::Duration::from_secs(if success { 50 } else { 10 }));
        }
    }

    pub fn failed(&mut self, now: std::time::Instant) {
        self.generation = self.generation.wrapping_add(1);
        self.result = Some(false);
        self.expires = None;
        self.retry = Some(now + std::time::Duration::from_secs(10));
    }

    pub fn reason(&self, screen: bool, input: bool) -> Option<&'static str> {
        capture_reason(screen, input, self.result)
    }
}

pub const FRAME_WAIT: &str = "Waiting for a screen frame on that Mac";
pub const FRAME_FAILED: &str =
    "No screen frame arrived. Check the display and Screen Recording on that Mac.";

pub fn capture_reason(screen: bool, input: bool, frame: Option<bool>) -> Option<&'static str> {
    if !screen || !input {
        return Some(WAITING);
    }
    match frame {
        None => Some(FRAME_WAIT),
        Some(false) => Some(FRAME_FAILED),
        Some(true) => None,
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    #[test]
    fn revoked_inflight_frame_and_bounded_retry() {
        let now = std::time::Instant::now();
        let mut check = FrameCheck::new();
        let initial = check.begin(true, now).unwrap();
        assert!(check.begin(true, now).is_none());
        check.begin(false, now);
        check.complete(initial, true, now);
        assert_eq!(check.reason(true, true), Some(FRAME_WAIT));
        let retry = check.begin(true, now).unwrap();
        check.complete(retry, false, now);
        assert_eq!(check.reason(true, true), Some(FRAME_FAILED));
        assert!(check.begin(true, now).is_none());
        let later = now + std::time::Duration::from_secs(10);
        let recovered = check.begin(true, later).unwrap();
        check.complete(recovered, true, later);
        assert_eq!(check.reason(true, true), None);
        check.failed(later);
        assert_eq!(check.reason(true, true), Some(FRAME_FAILED));
    }
    #[test]
    fn expired_success_requires_a_new_frame() {
        let now = std::time::Instant::now();
        let mut check = FrameCheck::new();
        let generation = check.begin(true, now).unwrap();
        check.complete(generation, true, now);
        let later = now + std::time::Duration::from_secs(60);
        let next = check.begin(true, later).expect("old observation expired");
        assert_eq!(check.reason(true, true), Some(FRAME_WAIT));
        check.complete(next, false, later);
        assert_eq!(check.reason(true, true), Some(FRAME_FAILED));
    }

    #[test]
    fn active_viewer_can_stream_while_a_fresh_observation_is_renewed() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let mut check = FrameCheck::new();
        let first = check.begin(true, now).unwrap();
        check.complete(first, true, now);
        let refreshing = now + Duration::from_secs(50);
        let second = check.begin(true, refreshing).unwrap();
        assert_eq!(check.reason(true, true), None);
        // Even an indefinitely stalled check cannot extend the old result.
        assert!(check.begin(true, now + Duration::from_secs(60)).is_none());
        assert_eq!(check.reason(true, true), Some(FRAME_WAIT));
        check.complete(second, true, now + Duration::from_secs(61));
        assert_eq!(check.reason(true, true), None);
    }

    #[test]
    fn grants_without_frame() {
        assert_eq!(capture_reason(true, true, None), Some(FRAME_WAIT));
        assert_eq!(capture_reason(true, true, Some(false)), Some(FRAME_FAILED));
        assert_eq!(capture_reason(true, true, Some(true)), None);
        assert_eq!(capture_reason(false, true, Some(true)), Some(WAITING));
    }

    #[test]
    fn late_tailscale() {
        assert!(retry_tail(None, "lan", true));
        assert!(!retry_tail(None, "lan", false));
        assert!(!retry_tail(Some("lan"), "lan", true));
        assert!(!retry_tail(None, "tailscale", true));
        assert_eq!(
            listen_setting(None, Some("100.64.1.1".parse().unwrap()), None)
                .unwrap()
                .1,
            "tailscale"
        );
    }
    #[cfg(unix)]
    #[test]
    fn paused_across_two_updates() {
        use std::fs;
        let root = std::env::temp_dir().join(format!("sunna-plist-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let agent = root.join("agent.plist");
        for version in ["one", "two", "three"] {
            let bundle = root.join(version);
            fs::create_dir(&bundle).unwrap();
            let program = bundle.join("Sunna");
            fs::write(&program, version).unwrap();
            let desired = plist(program.to_str().unwrap(), "/log");
            if version == "one"
                || paused_update(&fs::read_to_string(&agent).unwrap(), &desired, true)
            {
                fs::write(&agent, desired).unwrap();
            }
        }
        fs::remove_dir_all(root.join("one")).unwrap();
        let xml = fs::read_to_string(agent).unwrap();
        let program = xml
            .split_once("<key>ProgramArguments</key><array><string>")
            .unwrap()
            .1
            .split_once("</string>")
            .unwrap()
            .0;
        assert_eq!(fs::read_to_string(program).unwrap(), "three");
        fs::remove_dir_all(root).unwrap();
    }
}
