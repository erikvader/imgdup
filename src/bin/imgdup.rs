use std::{
    cmp,
    collections::HashSet,
    ffi::OsString,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::{mpsc, Mutex},
    thread,
    time::{Duration, Instant},
};

use clap::Parser;
use color_eyre::eyre::{self, Context};
use image::RgbImage;
use imgdup::{
    bktree::BKTree,
    common::{init_eyre, init_logger, Mirror, VidSrc},
    frame_extractor::{timestamp::Timestamp, FrameExtractor},
    fsutils::{all_files, is_simple_relative, read_optional_file},
    imghash::{
        self,
        hamming::{Distance, Hamming},
    },
    imgutils::{self, RemoveBordersConf},
    repo::{LazyEntry, Repo},
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

    /// Use this many threads reading video files
    #[arg(long, short = 'j', default_value = "1")]
    video_threads: NonZeroU32,

    /// Only process up to this many new video files
    #[arg(long, default_value_t = usize::MAX)]
    limit: usize,

    /// A file to additionally write the logs to
    #[arg(long)]
    logfile: Option<PathBuf>,

    /// Folder of pictures to ignore
    #[arg(long, short = 'i')]
    ignore_dir: Option<PathBuf>,

    /// Folder to place filtered out pictures and other debug info
    #[arg(long, short = 'g')]
    graveyard_dir: Option<PathBuf>,

    /// Where to place the results
    #[arg(long, short = 'd', value_parser = simple_path_parser)]
    dup_dir: PathBuf,

    /// Folders with files to find duplicates among
    #[arg(long, short = 's', required = true, num_args=1.., value_parser = simple_path_parser)]
    src_dirs: Vec<PathBuf>,

    /// Path to the database to use
    #[arg(long, short = 'f', default_value = "./imgdup.db")]
    database_file: PathBuf,
}

fn simple_path_parser(s: &str) -> Result<PathBuf, String> {
    if is_simple_relative(s) {
        Ok(s.into())
    } else {
        Err(format!(
            "path is not simple relative, i.e., is relative and only contains \
                     normal components"
        ))
    }
}

fn cli_arguments() -> eyre::Result<Cli> {
    const ARGS_FILE: &str = ".imgduprc";
    let mut args: Vec<OsString> = std::env::args_os().collect();

    if args.len() == 1 {
        if let Some(flags) = read_optional_file(ARGS_FILE)
            .wrap_err_with(|| format!("Could not read config file at: {ARGS_FILE}"))?
        {
            args.extend(
                flags
                    .split_whitespace()
                    .map(|s| std::ffi::OsStr::new(s).to_owned()),
            );
        }
    }

    Ok(Cli::parse_from(args))
}

fn main() -> eyre::Result<()> {
    init_eyre()?;
    let cli = cli_arguments()?;
    init_logger(cli.logfile.as_deref())?;

    let mut tree =
        BKTree::<VidSrc>::from_file(&cli.database_file).wrap_err_with(|| {
            format!(
                "failed to open database at: {}",
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
            tree_files.insert(src.path().to_owned());
        })?;
        tree_files
    };
    log::info!("Found {} files", tree_files.len());

    let new_files: Vec<_> = src_files
        .difference(&tree_files)
        .take(cli.limit)
        .map(|pb| pb.as_path())
        .collect();
    let removed_files: HashSet<_> = tree_files
        .difference(&src_files)
        .map(|pb| pb.as_path())
        .collect();

    log::info!("Removing {} removed files from the DB", removed_files.len());
    tree.remove_any_of(|vidsrc| removed_files.contains(vidsrc.path()))?;

    let video_conf = VideoWorkerConf::default()
        .similarity_threshold(cli.similarity_threshold)
        .border_conf(
            RemoveBordersConf::default()
                .maskify_threshold(cli.maskify_threshold)
                .maximum_whites(cli.maximum_whites),
        );

    let ignored_hashes = if let Some(ignore_dir) = cli.ignore_dir {
        log::info!("Reading images to ignore from: {}", ignore_dir.display());
        read_ignored(ignore_dir, &video_conf)?
    } else {
        vec![]
    };
    log::info!("Ignoring {} images", ignored_hashes.len());

    log::info!("Processing {} new files", new_files.len());
    let work_queue = WorkQueue::new(new_files);
    let repo_dup = Repo::new(cli.dup_dir).wrap_err("failed to create the dup repo")?;
    let repo_grave = if let Some(grave) = cli.graveyard_dir {
        Some(Mutex::new(
            Repo::new(grave).wrap_err("failed to create graveyard repo")?,
        ))
    } else {
        None
    };

    thread::scope(|s| {
        let handles = {
            let mut handles = vec![];
            let (tx, rx) = mpsc::sync_channel::<Payload>(16);

            for i in 0..cmp::min(work_queue.len(), cli.video_threads.get() as usize) {
                let tx = tx.clone();
                let video_thread = thread::Builder::new()
                    .name(format!("V{i:03}"))
                    .spawn_scoped(s, || {
                        video_worker(
                            tx,
                            &work_queue,
                            &video_conf,
                            &ignored_hashes,
                            repo_grave.as_ref(),
                        )
                    })
                    .expect("failed to spawn thread");
                handles.push((format!("video_{i}"), video_thread));
            }

            let tree_thread = thread::Builder::new()
                .name(format!("Tree"))
                .spawn_scoped(s, || {
                    tree_worker(rx, tree, repo_dup, cli.similarity_threshold)
                })
                .expect("failed to spawn thread");
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
    video_path: &'env Path,
    hashes: Vec<(Timestamp, Hamming, Mirror)>,
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
    ignored_hashes: &[Hamming],
    repo_graveyard: Option<&Mutex<Repo>>,
) -> eyre::Result<()> {
    log::debug!("Video worker at your service");
    while let Some((i, video)) = videos.next_index() {
        log::info!("Progress: {}/{} videos", i + 1, videos.len());

        match get_hashes(video, ignored_hashes, config, repo_graveyard) {
            Err(e) => {
                log::error!("Failed to get hashes from '{}': {:?}", video.display(), e)
            }
            Ok(hashes) => {
                let load = Payload {
                    video_path: video,
                    hashes,
                };

                if let Err(load) = match tx.try_send(load) {
                    Ok(()) => Ok(()),
                    Err(mpsc::TrySendError::Full(load)) => {
                        log::warn!("The channel is full");
                        Err(load)
                    }
                    Err(mpsc::TrySendError::Disconnected(load)) => Err(load),
                } {
                    if let Err(_) = tx.send(load) {
                        eyre::bail!("The receiver is down");
                    }
                }
            }
        }
    }
    log::debug!("Video worker done");
    Ok(())
}

fn get_hashes(
    video: &Path,
    ignored_hashes: &[Hamming],
    config: &VideoWorkerConf,
    repo_graveyard: Option<&Mutex<Repo>>,
) -> eyre::Result<Vec<(Timestamp, Hamming, Mirror)>> {
    log::info!("Retrieving hashes for: {}", video.display());
    let mut extractor =
        FrameExtractor::new(video).wrap_err("Failed to create the extractor")?;
    let approx_len = extractor.approx_length();

    // TODO: move to `config`
    let min_frames: NonZeroU32 = NonZeroU32::new(5).unwrap();
    let max_step: Duration = Duration::from_secs(10);
    let log_every = Duration::from_secs(10);

    let step = calc_step(approx_len, min_frames, max_step);
    log::debug!("Stepping with {}s", step.as_secs_f64());

    let mut hashes = Vec::with_capacity(estimated_num_of_frames(approx_len, step));
    let approx_len = Timestamp::duration_to_string(approx_len);

    let mut graveyard_entry = LazyEntry::new();

    let mut last_logged = Instant::now();
    while let Some((ts, frame)) = extractor.next().wrap_err("Failed to get a frame")? {
        let now = Instant::now();
        if now - last_logged >= log_every {
            last_logged = now;
            log::debug!("At timestamp: {}/{}", ts.to_string(), approx_len);
        }

        use FrameToHashResult as F;
        match frame_to_hash(
            &frame,
            ignored_hashes,
            config,
            hashes.last().map(|(_, h, _)| *h),
        ) {
            F::Ok(hash) => {
                hashes.push((ts.clone(), hash, Mirror::Normal));
                let mirror = imgutils::mirror(frame);
                if let F::Ok(hash) =
                    frame_to_hash(&mirror, ignored_hashes, config, Some(hash))
                {
                    hashes.push((ts, hash, Mirror::Mirrored));
                }
            }
            res @ F::Ignored | res @ F::Empty if repo_graveyard.is_some() => {
                let entry = graveyard_entry.get_or_init(|| {
                    let mut entry =
                        repo_graveyard.unwrap().lock().unwrap().new_entry()?;
                    entry.create_link_relative("original", video)?;
                    Ok(entry)
                })?;

                entry.create_jpg(
                    format!("{}_{}.jpg", res.name(), ts.to_string()),
                    &frame,
                )?;
            }
            _ => (),
        }

        extractor.seek_forward(step).wrap_err("Failed to seek")?;
    }

    log::info!("Got {} hashes from: {}", hashes.len(), video.display());
    Ok(hashes)
}

enum FrameToHashResult {
    Empty,
    Ignored,
    TooSimilarToPrevious,
    Ok(Hamming),
}

impl FrameToHashResult {
    fn name(&self) -> &'static str {
        match self {
            FrameToHashResult::Empty => "empty",
            FrameToHashResult::Ignored => "ignored",
            FrameToHashResult::TooSimilarToPrevious => "similar_previous",
            FrameToHashResult::Ok(_) => "ok",
        }
    }
}

fn frame_to_hash(
    frame: &RgbImage,
    ignored_hashes: &[Hamming],
    config: &VideoWorkerConf,
    last_hash: Option<Hamming>,
) -> FrameToHashResult {
    let frame = imgutils::remove_borders(&frame, &config.border_conf);

    if imgutils::is_subimg_empty(&frame) {
        return FrameToHashResult::Empty;
    }

    let hash = imghash::hash_sub(&frame);

    if ignored_hashes
        .iter()
        .any(|ignore| ignore.distance_to(hash) <= config.similarity_threshold)
    {
        return FrameToHashResult::Ignored;
    }

    match last_hash {
        Some(last_hash) if hash.distance_to(last_hash) > config.similarity_threshold => {
            FrameToHashResult::Ok(hash)
        }
        None => FrameToHashResult::Ok(hash),
        _ => FrameToHashResult::TooSimilarToPrevious,
    }
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

fn read_ignored(
    dir: impl AsRef<Path>,
    conf: &VideoWorkerConf,
) -> eyre::Result<Vec<Hamming>> {
    let all_files: Vec<_> = all_files([dir]).wrap_err("failed to read dir")?;
    let mut hashes = Vec::with_capacity(all_files.len());
    let mut hashes_path = Vec::with_capacity(all_files.len());
    let mut hashes_mirrored: Vec<bool> = Vec::with_capacity(all_files.len());

    for file in all_files.iter() {
        let mut img = image::open(&file)
            .wrap_err_with(|| format!("could not open {} as an image", file.display()))?
            .to_rgb8();

        for mirrored in [false, true] {
            if mirrored {
                img = imgutils::mirror(img);
            }

            let img = imgutils::remove_borders(&img, &conf.border_conf);
            if imgutils::is_subimg_empty(&img) {
                log::warn!(
                    "The ignored file '{}' is empty after border removal (mirror={mirrored})",
                    file.display()
                );
                continue;
            }

            let hash = imghash::hash_sub(&img);
            let the_same: Vec<_> = hashes
                .iter()
                .enumerate()
                .filter(|(_, ignore)| {
                    hash.distance_to(**ignore) <= conf.similarity_threshold
                })
                .filter(|(i, _)| !hashes_mirrored[*i])
                .map(|(i, _)| &hashes_path[i])
                .filter(|coll_path| coll_path != &&file)
                .collect();

            if !the_same.is_empty() {
                log::warn!(
                    "The ignored file '{}' (mirrored={mirrored}) is the same as: {:?}",
                    file.display(),
                    the_same,
                );
                continue;
            }

            hashes.push(hash);
            hashes_path.push(file);
            hashes_mirrored.push(mirrored);
        }
    }

    Ok(hashes)
}

// TODO: handle ctrl+c and properly close the db
fn tree_worker(
    rx: mpsc::Receiver<Payload>,
    mut tree: BKTree<VidSrc>,
    mut repo: Repo,
    similarity_threshold: Distance,
) -> eyre::Result<()> {
    log::debug!("Tree worker working");

    while let Ok(Payload { video_path, hashes }) = rx.recv() {
        log::info!("Finding dups of: {}", video_path.display());
        let similar_videos = find_similar_videos(
            hashes.iter().map(|(_, hash, _)| *hash),
            &mut tree,
            similarity_threshold,
        )
        .wrap_err("failed to find similar videos")?;
        log::info!("Found {} duplicate videos", similar_videos.len());

        if !similar_videos.is_empty() {
            link_dup(&mut repo, video_path, similar_videos)
                .wrap_err("failed to link dup")?;
        }

        log::info!("Saving {} hashes", hashes.len());
        save_video(hashes, &mut tree, video_path)
            .wrap_err("failed to save some video hashes to the tree")?;
        log::info!("Done saving");
    }

    log::info!("Closing the tree");
    tree.close().wrap_err("failed to close the tree")?;
    log::info!("Closed!");

    log::debug!("Tree worker not working");
    Ok(())
}

fn save_video(
    hashes: Vec<(Timestamp, Hamming, Mirror)>,
    tree: &mut BKTree<VidSrc>,
    video_path: &Path,
) -> eyre::Result<()> {
    tree.add_all(hashes.into_iter().map(|(ts, hash, mirrored)| {
        (hash, VidSrc::new(ts, video_path.to_owned(), mirrored))
    }))
    .wrap_err("failed to add to the tree")?;

    Ok(())
}

fn link_dup(
    repo: &mut Repo,
    video_path: &Path,
    similar_videos: HashSet<PathBuf>,
) -> eyre::Result<()> {
    let mut entry = repo
        .new_entry()
        .wrap_err("failed to create repo entry for a new dup")?;

    entry
        .create_link_relative("the_new_one", video_path)
        .wrap_err("failed to link the new one")?;

    for similar in similar_videos.into_iter() {
        entry
            .create_link_relative("dup", similar)
            .wrap_err("failed to link a dup")?;
    }

    Ok(())
}

fn find_similar_videos(
    hashes: impl IntoIterator<Item = Hamming>,
    tree: &mut BKTree<VidSrc>,
    similarity_threshold: Distance,
) -> eyre::Result<HashSet<PathBuf>> {
    let mut similar_videos = HashSet::new();
    for hash in hashes {
        tree.find_within(hash, similarity_threshold, |_, src| {
            similar_videos.insert(src.path().to_owned());
        })
        .wrap_err("failed to find_within")?;
    }
    Ok(similar_videos)
}
