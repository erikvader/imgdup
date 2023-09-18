use std::{
    fmt,
    fs::File,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{self, Context};

use crate::{frame_extractor::timestamp::Timestamp, fsutils::is_simple_relative};

#[derive(serde::Serialize, serde::Deserialize, Clone, Hash, PartialEq, Eq)]
pub struct VidSrc {
    frame_pos: Timestamp,
    path: PathBuf,
    mirrored: Mirror,
}

#[derive(
    serde::Serialize, serde::Deserialize, Copy, Clone, Hash, PartialEq, Eq, Debug,
)]
pub enum Mirror {
    Normal,
    Mirrored,
}

impl fmt::Display for Mirror {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self, f)
    }
}

impl VidSrc {
    pub fn new(frame_pos: Timestamp, path: PathBuf, mirrored: Mirror) -> Self {
        assert!(is_simple_relative(&path));
        Self {
            frame_pos,
            path,
            mirrored,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn frame_pos(&self) -> &Timestamp {
        &self.frame_pos
    }

    pub fn mirrored(&self) -> Mirror {
        self.mirrored
    }
}

pub fn init_logger_and_eyre() -> eyre::Result<()> {
    use color_eyre::config::{HookBuilder, Theme};
    use simplelog::*;

    let mut builder = ConfigBuilder::new();
    builder.set_thread_level(LevelFilter::Error);
    builder.set_target_level(LevelFilter::Error);
    builder.set_location_level(LevelFilter::Trace);

    builder.set_level_padding(LevelPadding::Right);
    builder.set_thread_padding(ThreadPadding::Right(4));

    builder.set_thread_mode(ThreadLogMode::Both);

    // NOTE: set_time_offset_to_local can only be run when there is only on thread active.
    let timezone_failed = builder.set_time_offset_to_local().is_err();

    let level = LevelFilter::Debug;
    let (log_color, eyre_color) = if std::io::IsTerminal::is_terminal(&std::io::stdout())
    {
        (ColorChoice::Auto, Theme::dark())
    } else {
        (ColorChoice::Never, Theme::new())
    };

    HookBuilder::default()
        .theme(eyre_color)
        .install()
        .wrap_err("Failed to install eyre")?;

    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![TermLogger::new(
        level,
        builder.build(),
        TerminalMode::Stdout,
        log_color,
    )];

    // TODO: a way to disable per application? Create a unique one for each?
    const LOGFILE: &str = "/tmp/imgdup.log";
    let logfile_failed = match File::create(LOGFILE) {
        Ok(f) => {
            loggers.push(WriteLogger::new(level, builder.build(), f));
            None
        }
        Err(e) => Some(e),
    };

    CombinedLogger::init(loggers).wrap_err("Failed to set the logger")?;

    if timezone_failed {
        log::error!(
            "Failed to set time zone for the logger, using UTC instead (I think)"
        );
    }

    if let Some(e) = logfile_failed {
        log::error!("Failed to create the log file at '{LOGFILE}' because: {e}");
    } else {
        log::debug!("Logging to: {LOGFILE}");
    }

    Ok(())
}
