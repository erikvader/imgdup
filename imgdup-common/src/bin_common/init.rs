use std::path::Path;

use color_eyre::{
    config::{HookBuilder, Theme},
    eyre::{self, Context},
};
use owo_colors::{OwoColorize, Style};
use time::{OffsetDateTime, UtcOffset};

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
    let mut dispatch = fern::Dispatch::new().level(log::LevelFilter::Trace).chain(
        fern::Dispatch::new()
            .format(format_callback(
                supports_color::on(supports_color::Stream::Stdout)
                    .is_some_and(|support| support.has_basic),
            ))
            .chain(std::io::stdout()),
    );

    if let Some(logfile) = logfile {
        dispatch =
            dispatch.chain(fern::Dispatch::new().format(format_callback(false)).chain(
                fern::log_file(logfile).wrap_err_with(|| {
                    format!("failed to open the log file at: {logfile:?}")
                })?,
            ));
    }

    dispatch.apply().wrap_err("failed to set the logger")?;

    Ok(())
}

fn format_callback(
    color: bool,
) -> impl Fn(fern::FormatCallback<'_>, &std::fmt::Arguments<'_>, &log::Record<'_>)
       + Sync
       + Send
       + 'static {
    let utc_offset = match UtcOffset::current_local_offset() {
        Ok(offset) => offset,
        Err(e) => {
            eprintln!("Failed to get the current UTC offset: {e:?}");
            UtcOffset::UTC
        }
    };
    let format = time::macros::format_description!(
        "[hour repr:24]:[minute]:[second].[subsecond digits:6]"
    );

    move |out, message, record| {
        let now = OffsetDateTime::now_utc().to_offset(utc_offset).time();
        let style = if color {
            level_style(record.level())
        } else {
            Style::new()
        };

        out.finish(format_args!(
            "{} ({}) [{:<5}] {}: {}",
            now.format(format)
                .unwrap_or_else(|_| "??:??:??.??????".into()),
            std::thread::current().name().unwrap_or("??"),
            record.level().to_string().style(style),
            record.target(),
            message.style(style),
        ))
    }
}

fn level_style(level: log::Level) -> Style {
    match level {
        log::Level::Error => Style::new().bright_red().bold(),
        log::Level::Warn => Style::new().bright_yellow().bold(),
        log::Level::Info => Style::new().bright_white().bold(),
        log::Level::Debug => Style::new().white(),
        log::Level::Trace => Style::new().dimmed(),
    }
}
