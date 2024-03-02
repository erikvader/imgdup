use std::path::Path;

use color_eyre::{
    config::{HookBuilder, Theme},
    eyre::{self, Context},
};
use fern_format::{Format, Stream};

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
            .format(
                Format::new()
                    .color_if_supported(Stream::Stdout)
                    .uniquely_color_threads()
                    .callback(),
            )
            .chain(std::io::stdout()),
    );

    if let Some(logfile) = logfile {
        dispatch = dispatch.chain(
            fern::Dispatch::new()
                .format(Format::new().thread_names().callback())
                .chain(fern::log_file(logfile).wrap_err_with(|| {
                    format!("failed to open the log file at: {logfile:?}")
                })?),
        );
    }

    dispatch.apply().wrap_err("failed to set the logger")?;

    Ok(())
}
