//! Logs to logcat, tagged "sunna" (`adb logcat -s sunna`).

use std::ffi::{c_char, c_int, CString};

use tracing::{Level, Metadata};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::EnvFilter;

#[link(name = "log")]
extern "C" {
    fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

struct Logcat;

/// One log line, written out when the formatter is done with it.
struct Line {
    priority: c_int,
    text: Vec<u8>,
}

impl std::io::Write for Line {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.text.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for Line {
    fn drop(&mut self) {
        let mut text = std::mem::take(&mut self.text);
        while text.last() == Some(&b'\n') {
            text.pop();
        }
        text.retain(|&byte| byte != 0);
        if let Ok(text) = CString::new(text) {
            // SAFETY: both strings are NUL-terminated.
            unsafe { __android_log_write(self.priority, c"sunna".as_ptr(), text.as_ptr()) };
        }
    }
}

impl<'a> MakeWriter<'a> for Logcat {
    type Writer = Line;

    fn make_writer(&'a self) -> Line {
        Line { priority: 4, text: Vec::new() }
    }

    fn make_writer_for(&'a self, meta: &Metadata<'_>) -> Line {
        // android/log.h: VERBOSE 2, DEBUG 3, INFO 4, WARN 5, ERROR 6.
        let priority = match *meta.level() {
            Level::TRACE => 2,
            Level::DEBUG => 3,
            Level::INFO => 4,
            Level::WARN => 5,
            Level::ERROR => 6,
        };
        Line { priority, text: Vec::new() }
    }
}

pub fn init() {
    let _ = tracing_subscriber::fmt()
        .with_writer(Logcat)
        .with_ansi(false)
        .without_time()
        .with_env_filter(EnvFilter::new("info,quinn=warn,quinn_proto=warn,rustls=warn"))
        .try_init();
}
