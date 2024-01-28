use std::{fmt::Arguments, path::Path};

pub struct Item {
    pub level: Level,
    pub target: String,
    pub body: String,
}

pub trait Logger {
    fn log(&self, level: Level, target: &str, body: Arguments<'_>);
    fn log_item(&self, item: Item) {
        self.log(item.level, &item.target, format_args!("{}", item.body))
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Level {
    Verbose,
    Info,
    Warn,
    Error,
}

pub struct LogLogger;

impl Logger for LogLogger {
    fn log(&self, level: Level, target: &str, body: Arguments<'_>) {
        let level = match level {
            Level::Verbose => log::Level::Debug,
            Level::Info => log::Level::Info,
            Level::Warn => log::Level::Warn,
            Level::Error => log::Level::Error,
        };
        log::log!(target: target, level, "{}", body);
    }
}

pub struct ContextLogger<'a> {
    video: &'a Path,
}

impl<'a> Logger for ContextLogger<'a> {
    fn log(&self, level: Level, target: &str, body: Arguments<'_>) {
        let logger = LogLogger;
        logger.log(
            level,
            target,
            format_args!("{} ({})", body, self.video.display()),
        )
    }
}

impl<'a> ContextLogger<'a> {
    pub fn new(video: &'a Path) -> Self {
        Self { video }
    }
}

#[allow(unused_macros)]
macro_rules! information {
    ($logger:expr, $($args:tt),* $(,)*) => {
        $logger.log(
            $crate::frame_extractor::logger::Level::Info,
            std::module_path!(),
            std::format_args!($($args),*)
        )
    }
}

#[allow(unused_macros)]
macro_rules! warning {
    ($logger:expr, $($args:tt),* $(,)*) => {
        $logger.log(
            $crate::frame_extractor::logger::Level::Warn,
            std::module_path!(),
            std::format_args!($($args),*)
        )
    }
}

#[allow(unused_macros)]
macro_rules! fault {
    ($logger:expr, $($args:tt),* $(,)*) => {
        $logger.log(
            $crate::frame_extractor::logger::Level::Error,
            std::module_path!(),
            std::format_args!($($args),*)
        )
    }
}

#[allow(unused_macros)]
macro_rules! verbose {
    ($logger:expr, $($args:tt),* $(,)*) => {
        $logger.log(
            $crate::frame_extractor::logger::Level::Verbose,
            std::module_path!(),
            std::format_args!($($args),*)
        )
    }
}

#[allow(unused_imports)]
pub(crate) use fault;
#[allow(unused_imports)]
pub(crate) use information;
#[allow(unused_imports)]
pub(crate) use verbose;
#[allow(unused_imports)]
pub(crate) use warning;
