//! In-memory log shown by the `:log` command. Also the sink for the `log` facade.

use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::Instant;

use parking_lot::Mutex;

const CAPACITY: usize = 1_000;

#[derive(Clone, Debug)]
pub struct LogLine {
    pub at: Instant,
    pub level: log::Level,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct LogBuffer {
    lines: Mutex<VecDeque<LogLine>>,
}

static GLOBAL: OnceLock<LogBuffer> = OnceLock::new();

impl LogBuffer {
    pub fn global() -> &'static LogBuffer {
        GLOBAL.get_or_init(LogBuffer::default)
    }

    pub fn push(&self, level: log::Level, message: impl Into<String>) {
        let mut lines = self.lines.lock();
        if lines.len() >= CAPACITY {
            lines.pop_front();
        }
        lines.push_back(LogLine { at: Instant::now(), level, message: message.into() });
    }

    pub fn snapshot(&self) -> Vec<LogLine> {
        self.lines.lock().iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.lines.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.lock().is_empty()
    }
}

struct BufferLogger;

impl log::Log for BufferLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Debug
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            LogBuffer::global().push(record.level(), record.args().to_string());
        }
    }

    fn flush(&self) {}
}

static LOGGER: BufferLogger = BufferLogger;

/// Routes the `log` crate into the buffer. Safe to call once at startup.
pub fn install() {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
}
