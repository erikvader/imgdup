use std::{fs::File, path::Path};

use color_eyre::{
    config::{HookBuilder, Theme},
    eyre::{self, Context},
};
use simplelog::*;

pub fn init_eyre() -> eyre::Result<()> {
    let eyre_color = if std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        Theme::dark()
    } else {
        Theme::new()
    };

    let (stderr_panic_hook, eyre_hook) =
        HookBuilder::default().theme(eyre_color).into_hooks();
    eyre_hook
        .install()
        .wrap_err("failed to install eyre hook")?;

    let (log_panic_hook, _) = HookBuilder::default().theme(Theme::new()).into_hooks();

    std::panic::set_hook(Box::new(move |info| {
        // NOTE: The default: https://docs.rs/color-eyre/0.6.2/src/color_eyre/config.rs.html#981
        eprintln!("{}", stderr_panic_hook.panic_report(info));

        log::error!(target: "panic", "{}", log_panic_hook.panic_report(info));
    }));

    Ok(())
}

pub fn init_logger(logfile: Option<&Path>) -> eyre::Result<()> {
    let mut builder = ConfigBuilder::new();
    builder.set_thread_level(LevelFilter::Error);
    builder.set_target_level(LevelFilter::Error);
    builder.set_location_level(LevelFilter::Trace);

    builder.set_level_padding(LevelPadding::Right);
    builder.set_thread_padding(ThreadPadding::Right(3));

    builder.set_thread_mode(ThreadLogMode::Both);

    // NOTE: set_time_offset_to_local can only be run when there is only on thread active.
    let timezone_failed = builder.set_time_offset_to_local().is_err();

    let level = LevelFilter::Debug;
    let log_color = if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    };

    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![TermLogger::new(
        level,
        builder.build(),
        TerminalMode::Stdout,
        log_color,
    )];

    let logfile_failed = logfile.and_then(|logfile| match File::create(logfile) {
        Ok(f) => {
            loggers.push(WriteLogger::new(level, builder.build(), f));
            None
        }
        Err(e) => Some(e),
    });

    CombinedLogger::init(loggers).wrap_err("Failed to set the logger")?;

    if timezone_failed {
        log::error!(
            "Failed to set time zone for the logger, using UTC instead (I think)"
        );
    }

    if let Some(logfile) = logfile {
        if let Some(e) = logfile_failed {
            log::error!(
                "Failed to create the log file at '{}' because: {e}",
                logfile.display()
            );
        } else {
            log::debug!("Logging to: {}", logfile.display());
        }
    }

    Ok(())
}
