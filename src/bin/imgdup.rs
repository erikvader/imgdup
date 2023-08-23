use std::{
    collections::HashSet,
    convert::identity,
    ffi::OsString,
    panic::resume_unwind,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use clap::Parser;
use color_eyre::eyre::{self, Context};
use imgdup::{
    bktree::BKTree,
    frame_extractor::{timestamp::Timestamp, FrameExtractor},
    fsutils::{all_files, is_dir_empty, read_optional_file},
    imghash::{
        self,
        hamming::{Distance, Hamming},
    },
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

    /// Path to the database to use
    #[arg(long, short = 'f')]
    database_file: PathBuf,
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

    log::debug!("Parsing arguments: {args:?}");
    let cli = Cli::parse_from(args);
    log::debug!("Parsed: {cli:?}");
    Ok(cli)
}

#[derive(serde::Serialize, serde::Deserialize)]
struct VidSrc {
    frame_pos: Timestamp,
    // TODO: figure out a way to not store the whole path for every single hash
    path: PathBuf,
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    init_logger();
    let cli = cli_arguments()?;

    if !is_dir_empty(cli.dup_dir)? {
        eyre::bail!("Dup dir is not empty, or doesn't exist");
    }

    let mut tree =
        BKTree::<VidSrc>::from_file(&cli.database_file).wrap_err_with(|| {
            format!(
                "Failed to open database at: {}",
                cli.database_file.display()
            )
        })?;

    log::info!("Finding all files in: {:?}", cli.src_dirs);
    let src_files: HashSet<PathBuf> = all_files(cli.src_dirs)?;
    log::info!("Found {} files", src_files.len());

    log::info!(
        "Finding all files in database at: {}",
        cli.database_file.display()
    );
    let tree_files: HashSet<PathBuf> = {
        let mut tree_files = HashSet::new();
        tree.for_each(|_, src| {
            // TODO: https://doc.rust-lang.org/std/collections/struct.HashSet.html#method.get_or_insert_owned
            tree_files.insert(src.path.clone());
        })?;
        tree_files
    };
    log::info!("Found {} files", tree_files.len());

    let new_files: Vec<_> = src_files.difference(&tree_files).collect();
    let removed_files: HashSet<_> = tree_files.difference(&src_files).collect();

    log::info!("Removing {} removed files from the DB", removed_files.len());
    tree.remove_any_of(|vidsrc| removed_files.contains(&vidsrc.path))?;

    thread::scope(|s| -> eyre::Result<()> {
        let (tx, rx) = mpsc::sync_channel::<Payload>(16);
        // TODO: spawn multiple video threads
        let video_thread =
            s.spawn(|| video_worker(tx, new_files.iter().map(|p| p.as_path())));
        let tree_thread = s.spawn(|| tree_worker(rx, tree));

        video_thread
            .join()
            .map_err(|panic| resume_unwind(panic))
            .and_then(identity)
            .wrap_err("Something in the video thread failed")?;

        tree_thread
            .join()
            .map_err(|panic| resume_unwind(panic))
            .and_then(identity)
            .wrap_err("Something in the tree thread failed")?;

        Ok(())
    })?;

    Ok(())
}

struct Payload<'env> {
    path: &'env Path,
    hashes: Vec<(Timestamp, Hamming)>,
}

fn video_worker<'env, I>(
    tx: mpsc::SyncSender<Payload<'env>>,
    videos: I,
) -> eyre::Result<()>
where
    I: IntoIterator<Item = &'env Path>,
{
    let payload = Payload {
        path: videos.into_iter().next().unwrap(),
        hashes: vec![],
    };
    tx.send(payload).unwrap();
    todo!()
}

fn tree_worker(
    rx: mpsc::Receiver<Payload>,
    mut tree: BKTree<VidSrc>,
) -> eyre::Result<()> {
    let payload = rx.recv().unwrap();
    todo!()
}

// TODO: ligga i frame extractor?
// fn calc_step(video_length, minimum_frames, maximum_step) -> step;
// fn estimated_frames(video_length, step) -> number;
