use std::{
    collections::HashSet,
    ffi::OsString,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
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
    work_queue::WorkQueue,
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
    similarity_threshold: Distance,

    #[arg(long, short = 'j', default_value = "1")]
    video_threads: NonZeroU32,

    /// Folder of pictures to ignore
    #[arg(long, short = 'i')]
    ignore_dir: Option<PathBuf>,

    /// Folder to place filtered out pictures
    #[arg(long, short = 't')]
    trash_dir: Option<PathBuf>,

    /// Where to place the results
    #[arg(long, short = 'd')]
    dup_dir: PathBuf,

    /// Folders with files to find duplicates among
    #[arg(long, short = 's', required = true, num_args=1..)]
    src_dirs: Vec<PathBuf>,

    /// Path to the database to use
    #[arg(long, short = 'f')]
    database_file: PathBuf,
}

fn init_logger_and_eyre() -> eyre::Result<()> {
    use color_eyre::config::{HookBuilder, Theme};
    use simplelog::*;

    // TODO: always print thread number
    let mut builder = ConfigBuilder::new();
    // TODO: needed?
    // builder.add_filter_allow_str("imgdup");

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

    TermLogger::init(level, builder.build(), TerminalMode::Stdout, log_color)
        .wrap_err("Failed to set the logger")?;

    if timezone_failed {
        log::error!(
            "Failed to set time zone for the logger, using UTC instead (I think)"
        );
    }

    Ok(())
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
    init_logger_and_eyre()?;
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

    let new_files: Vec<_> = src_files
        .difference(&tree_files)
        .map(|pb| pb.as_path())
        .collect();
    let removed_files: HashSet<_> = tree_files.difference(&src_files).collect();

    log::info!("Removing {} removed files from the DB", removed_files.len());
    tree.remove_any_of(|vidsrc| removed_files.contains(&vidsrc.path))?;

    let video_conf = VideoWorkerConf::default()
        .similarity_threshold(cli.similarity_threshold)
        .border_conf(
            RemoveBordersConf::default()
                .maskify_threshold(cli.maskify_threshold)
                .maximum_whites(cli.maximum_whites),
        );

    let work_queue = WorkQueue::new(new_files);

    thread::scope(|s| {
        let handles = {
            let mut handles = vec![];
            let (tx, rx) = mpsc::sync_channel::<Payload>(16);

            for i in 0..cli.video_threads.get() {
                let tx = tx.clone();
                let video_thread = s.spawn(|| video_worker(tx, &work_queue, &video_conf));
                handles.push((format!("video_{i}"), video_thread));
            }

            let tree_thread = s.spawn(|| tree_worker(rx, tree));
            handles.push(("tree".to_string(), tree_thread));

            handles
        };

        for (name, handle) in handles {
            match handle.join() {
                Err(_) => log::error!("Thread '{name}' panicked"),
                Ok(Err(e)) => log::error!("Thread '{name}' returned an error: {e:?}"),
                Ok(Ok(())) => (),
            }
        }
    });

    Ok(())
}

#[derive(Debug)]
struct Payload<'env> {
    path: &'env Path,
    hashes: eyre::Result<Vec<(Timestamp, Hamming)>>,
}

struct VideoWorkerConf {
    border_conf: RemoveBordersConf,
    similarity_threshold: Distance,
}

impl Default for VideoWorkerConf {
    fn default() -> Self {
        Self {
            border_conf: RemoveBordersConf::default(),
            similarity_threshold: imghash::DEFAULT_SIMILARITY_THRESHOLD,
        }
    }
}

impl VideoWorkerConf {
    pub fn similarity_threshold(mut self, threshold: Distance) -> Self {
        self.similarity_threshold = threshold;
        self
    }

    pub fn border_conf(mut self, conf: RemoveBordersConf) -> Self {
        self.border_conf = conf;
        self
    }
}

fn video_worker<'env>(
    tx: mpsc::SyncSender<Payload<'env>>,
    videos: &WorkQueue<&'env Path>,
    config: &VideoWorkerConf,
) -> eyre::Result<()> {
    log::debug!("Video worker at your service");
    while let Some(video) = videos.next() {
        let load = Payload {
            path: video,
            hashes: get_hashes(video, config),
        };

        if let Err(_) = tx.send(load) {
            eyre::bail!("The receiver is down");
        }
    }
    log::debug!("Video worker done");
    Ok(())
}

fn get_hashes(
    video: &Path,
    config: &VideoWorkerConf,
) -> eyre::Result<Vec<(Timestamp, Hamming)>> {
    log::info!("Retrieving hashes for: {}", video.display());
    let mut extractor =
        FrameExtractor::new(video).wrap_err("Failed to create the extractor")?;
    let approx_len = extractor.approx_length();

    // TODO: move to `config`
    let min_frames: NonZeroU32 = NonZeroU32::new(5).unwrap();
    let max_step: Duration = Duration::from_secs(10);
    let step = calc_step(approx_len, min_frames, max_step);
    log::debug!("Stepping with {}s", step.as_secs_f64());

    let mut hashes = Vec::with_capacity(estimated_num_of_frames(approx_len, step));

    while let Some((ts, frame)) = extractor.next().wrap_err("Failed to get a frame")? {
        // log::debug!("At timestamp: {}/{}", ts.to_string(), approx_len.as_secs());
        // TODO: check for blacklisted images. Before or after border removal?
        let frame = imgutils::remove_borders(&frame, &config.border_conf);
        if imgutils::is_subimg_empty(&frame) {
            // TODO: save the original somewhere for later potential debugging
        } else {
            let hash = imghash::hash(&frame.to_image());
            let pushit = match hashes.last() {
                Some((_, last_hash)) => {
                    hash.distance_to(*last_hash) > config.similarity_threshold
                }
                None => true,
            };
            if pushit {
                hashes.push((ts, hash));
            }
        }

        extractor.seek_forward(step).wrap_err("Failed to seek")?;
    }

    Ok(hashes)
}

fn calc_step(
    video_length: Duration,
    minimum_frames: NonZeroU32,
    maximum_step: Duration,
) -> Duration {
    std::cmp::min(maximum_step, video_length / minimum_frames.get())
}

fn estimated_num_of_frames(video_length: Duration, step: Duration) -> usize {
    (video_length.as_secs_f64() / step.as_secs_f64()).ceil() as usize
}

fn tree_worker(
    rx: mpsc::Receiver<Payload>,
    mut tree: BKTree<VidSrc>,
) -> eyre::Result<()> {
    log::debug!("Tree worker working");
    while let Ok(payload) = rx.recv() {
        // dbg!(payload);
    }
    log::debug!("Tree worker not working");
    Ok(())
}
