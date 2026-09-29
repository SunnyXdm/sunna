//! What the in-session menu offers and what choosing an item does: shared by
//! the Mac's native menu (menu.rs) and the Linux viewer's (gpu.rs).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    ToggleStats,
    ToggleFullscreen,
    ToggleCapture,
    HideButton,
    Disconnect,
    Codec(Codec),
    /// Stream size as a percentage of this screen's.
    Scale(u8),
    BitrateMbps(u32),
    ToggleFastLane,
    /// One of the host's shortcuts (`send_keys::shortcuts_for`), by index.
    SendShortcut(u8),
    ToggleClipboard,
    TypeClipboard,
    FrameRate(u32),
    ToggleAudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Hevc,
    H264,
}

impl Codec {
    pub fn name(self) -> &'static str {
        match self {
            Codec::Hevc => "hevc",
            Codec::H264 => "h264",
        }
    }
}

pub const SCALES: [u8; 3] = [100, 75, 50];
pub const BITRATES_MBPS: [u32; 4] = [10, 20, 40, 80];
pub const FRAME_RATES: [u32; 2] = [60, 30];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuEvent {
    Chose(MenuAction),
    Closed,
}

/// What the menu shows checked or enabled.
#[derive(Clone)]
pub struct MenuState {
    /// Heading lines: the host, then what's streaming.
    pub host: String,
    pub detail: String,
    /// Shortcuts that suit the host's OS.
    pub shortcuts: &'static [crate::send_keys::Shortcut],
    /// Clipboard sharing is on.
    pub clipboard: bool,
    /// The stream's frame rate.
    pub fps: u32,
    pub stats: bool,
    pub fullscreen: bool,
    /// ⌘ shortcuts currently go to the remote.
    pub capture: bool,
    /// Keyboard capture is possible at all (Accessibility granted).
    pub capture_available: bool,
    pub button_visible: bool,
    /// The stream now: codec name, size, fast lane.
    pub codec: &'static str,
    pub stream_size: (u32, u32),
    pub fast_lane: bool,
    /// What the viewer asked for (None: the host's default).
    pub scale: u8,
    pub bitrate_mbps: Option<u32>,
    /// Stream settings can change live (the host supports it).
    pub video_available: bool,
    /// The host's sound is playing.
    pub audio: bool,
}
