use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use clap::Parser;
use color_eyre::eyre::{self, Context};
use imgdup::{
    frame_extractor::FrameExtractor,
    fsutils::read_optional_file,
    imghash::{self, hamming::Distance},
    imgutils::{self, RemoveBordersConf},
};

#[derive(Parser, Debug)]
#[command()]
/// Finds duplicate videos
struct Cli {
    /// All gray values below this becomes black
    #[arg(long, default_value_t = imgutils::DEFAULT_MASKIFY_THRESHOLD)]
    maskify_threshold: u8,

    /// A mask line can contain this many percent of white and still be considered black
    #[arg(long, default_value_t = imgutils::DEFAULT_BORDER_MAX_WHITES)]
    maximum_whites: f64,

    /// Maximum distance for two images to be considered equal
    #[arg(long, default_value_t = imghash::DEFAULT_SIMILARITY_THRESHOLD)]
    hamming_threshold: Distance,

    /// Folder of pictures to ignore
    #[arg(long, short = 'i')]
    ignore_dir: Option<PathBuf>,

    /// Folder to place filtered out pictures
    #[arg(long, short = 't')]
    trash_dir: Option<PathBuf>,

    /// Where to place the results
    #[arg(long, short = 'd')]
    dup_dir: PathBuf,

    /// Folders with files to find duplicates among, can be supplied multiple times
    #[arg(long, short = 's', required = true)]
    src_dirs: Vec<PathBuf>,
}

fn init_logger() {
    use simplelog::*;

    let mut builder = ConfigBuilder::new();
    // TODO: needed?
    // builder.add_filter_allow_str("imgdup");

    // NOTE: set_time_offset_to_local can only be run when there is only on thread active.
    if builder.set_time_offset_to_local().is_err() {
        eprintln!("Failed to set time zone for the logger, using UTC instead (I think)");
    }

    let level = LevelFilter::Debug;
    let colors = if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    };

    TermLogger::init(level, builder.build(), TerminalMode::Stdout, colors)
        .expect("could not init logger");
}

fn cli_arguments() -> eyre::Result<Cli> {
    const ARGS_FILE: &str = ".imgduprc";
    let mut args: Vec<OsString> = std::env::args_os().collect();

    if args.len() == 1 {
        if let Some(flags) =
            read_optional_file(ARGS_FILE).wrap_err("Could not read config file")?
        {
            args.extend(
                flags
                    .split_whitespace()
                    .map(|s| std::ffi::OsStr::new(s).to_owned()),
            );
        }
    }

    log::info!("Parsing arguments: {args:?}");
    Ok(Cli::parse_from(args))
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    init_logger();
    let cli = cli_arguments();

    println!("{cli:?}");

    Ok(())
}
