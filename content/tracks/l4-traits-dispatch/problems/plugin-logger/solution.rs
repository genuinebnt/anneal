use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

pub trait Logger {
    fn log(&self, level: Level, msg: &str);

    /// Pushes out anything buffered. Does nothing by default; every wrapper must pass it on.
    fn flush(&self) {}
}

/// A logger that can be shared across threads.
pub type BoxLogger = Box<dyn Logger + Send + Sync>;

/// Prepends `prefix` to every message.
pub struct PrefixLogger {
    prefix: String,
    inner: BoxLogger,
}

impl PrefixLogger {
    pub fn new(prefix: &str, inner: BoxLogger) -> Self {
        PrefixLogger {
            prefix: prefix.to_string(),
            inner,
        }
    }
}

impl Logger for PrefixLogger {
    fn log(&self, level: Level, msg: &str) {
        self.inner.log(level, &format!("{}{}", self.prefix, msg));
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

/// Drops messages below `min`.
pub struct LevelFilter {
    min: Level,
    inner: BoxLogger,
}

impl LevelFilter {
    pub fn new(min: Level, inner: BoxLogger) -> Self {
        LevelFilter { min, inner }
    }
}

impl Logger for LevelFilter {
    fn log(&self, level: Level, msg: &str) {
        if level >= self.min {
            self.inner.log(level, msg);
        }
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

/// Sends every message to all its sinks, in order.
pub struct Tee {
    sinks: Vec<BoxLogger>,
}

impl Tee {
    pub fn new(sinks: Vec<BoxLogger>) -> Self {
        Tee { sinks }
    }
}

impl Logger for Tee {
    fn log(&self, level: Level, msg: &str) {
        for s in &self.sinks {
            s.log(level, msg);
        }
    }

    fn flush(&self) {
        for s in &self.sinks {
            s.flush();
        }
    }
}

/// Lets one logger be shared by several wrappers.
impl<L: Logger + ?Sized> Logger for Arc<L> {
    fn log(&self, level: Level, msg: &str) {
        (**self).log(level, msg);
    }

    fn flush(&self) {
        (**self).flush();
    }
}
