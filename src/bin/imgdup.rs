use std::{
    collections::HashSet,
    ffi::OsString,
    num::NonZeroU32,
    path::PathBuf,
    sync::{mpsc, Mutex},
    time::{Duration, Instant},
};

use clap::Parser;
use color_eyre::eyre::{self, Context};
use common::Payload;
use image::RgbImage;
use imgdup::utils::{
    simple_path::SimplePath,
    work_queue::WorkQueue,
    workers::{scoped_workers, FinishedWorker},
};
use imgdup::{bin_common::ignored_hashes::read_ignored, imghash::preproc::PreprocArgs};
use imgdup::{bin_common::ignored_hashes::Ignored, imghash::similarity::SimiArgs};
use imgdup::{
    bin_common::init::{init_eyre, init_logger},
    bktree::{
        mmap::bktree::BKTree,
        source_types::video_source::{Mirror, VidSrc},
    },
    frame_extractor::{frame_extractor::FrameExtractor, timestamp::Timestamp},
    imghash::{hamming::Hamming, preproc::PreprocCli, similarity::SimiCli},
    utils::repo::{LazyEntry, Repo},
    utils::{
        fsutils::{all_files, read_optional_file},
        simple_path::clap_simple_relative_parser,
    },
    utils::{imgutils, simple_path::SimplePathBuf},
};
use imgdup::{imghash::preproc::PreprocError, utils::fsutils};
use rayon::prelude::*;

#[derive(Parser, Debug)]
#[command()]
/// Finds duplicate videos.
///
/// This uses rayon, so the `RAYON_NUM_THREADS` environment variable might be of interest.
struct Cli {
    #[command(flatten)]
    simi_args: SimiCli,

    #[command(flatten)]
    preproc_args: PreprocCli,

    /// Use this many threads to read video files
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
    #[arg(long, short = 'd', value_parser = clap_simple_relative_parser)]
    dup_dir: SimplePathBuf,

    /// Folders with files to find duplicates among
    #[arg(long, short = 's', required = true, num_args=1.., value_parser = clap_simple_relative_parser)]
    src_dirs: Vec<SimplePathBuf>,

    /// Path to the database to use
    #[arg(long, short = 'f', default_value = "./imgdup.db")]
    database_file: PathBuf,
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

    // TODO: extract all these functions
    log::info!("Backing up the database file");
    fsutils::backup_file(&cli.database_file)
        .wrap_err("failed to backup the database file")?;
    log::info!("Backed it up, if it existed");

    let mut tree =
        BKTree::<VidSrc>::from_file(&cli.database_file).wrap_err_with(|| {
            format!(
                "failed to open database at: {}",
                cli.database_file.display()
            )
        })?;

    log::info!("Finding all files in: {:?}", cli.src_dirs);
    let src_files: Vec<PathBuf> = all_files(cli.src_dirs)?;
    let src_files: Result<HashSet<SimplePathBuf>, _> = src_files
        .into_iter()
        .map(|path| SimplePathBuf::new(path))
        .collect();
    let src_files = src_files.wrap_err("some path from a src dir is not simple")?;
    log::info!("Found {} files", src_files.len());

    log::info!(
        "Finding all files in database at: {}",
        cli.database_file.display()
    );
    let tree_files: HashSet<SimplePathBuf> = {
        let mut tree_files = HashSet::new();
        tree.for_each(|_, src| {
            tree_files.insert(src.path().to_owned());
        })?;
        tree_files
    };
    log::info!("Found {} files", tree_files.len());

    let new_files: Vec<&SimplePath> = src_files
        .difference(&tree_files)
        .take(cli.limit)
        .map(|pb| pb.as_simple_path())
        .collect();
    let removed_files: HashSet<&SimplePath> = tree_files
        .difference(&src_files)
        .map(|pb| pb.as_simple_path())
        .collect();

    log::info!("Removing {} removed files from the DB", removed_files.len());
    tree.remove_any_of(|_, vidsrc| removed_files.contains(vidsrc.path()))?;

    let video_threads: usize = cli.video_threads.get().try_into().expect("should fit");
    let preproc_args = cli.preproc_args.to_args();
    let simi_args = cli.simi_args.to_args();

    let ignored_hashes = if let Some(ignore_dir) = cli.ignore_dir {
        log::info!("Reading images to ignore from: {}", ignore_dir.display());
        read_ignored(ignore_dir, &preproc_args, &simi_args)
            .wrap_err("failed to read images to ignore")?
    } else {
        Ignored::empty()
    };
    log::info!("Ignoring {} images", ignored_hashes.len());

    log::info!("Processing {} new files", new_files.len());
    let new_files = WorkQueue::new(new_files);

    let repo_dup = Repo::new(cli.dup_dir).wrap_err("failed to create the dup repo")?;
    let repo_grave = if let Some(grave) = cli.graveyard_dir {
        Some(Mutex::new(
            Repo::new(grave).wrap_err("failed to create graveyard repo")?,
        ))
    } else {
        None
    };

    let finished_workers = scoped_workers(|s| {
        let (tx, rx) = mpsc::sync_channel::<Payload>(16);

        let video_ctx = video::Ctx {
            preproc_args: &preproc_args,
            simi_args: &simi_args,
            ignored_hashes: &ignored_hashes,
            new_files: &new_files,
            repo_grave: repo_grave.as_ref(),
        };

        for _ in 0..video_threads {
            let tx = tx.clone();
            s.spawn("V", move || video::main(video_ctx, tx));
        }
        drop(tx);

        let tree_ctx = tree::Ctx {
            simi_args: &simi_args,
        };
        s.spawn("T", move || tree::main(tree_ctx, rx, tree, repo_dup));
    });

    for FinishedWorker { result, name } in finished_workers {
        match result {
            Err(panic) => log::error!("Thread '{name}' panicked with: {panic}"),
            Ok(Err(e)) => log::error!("Thread '{name}' returned an error: {e:?}"),
            Ok(Ok(())) => (),
        }
    }

    Ok(())
}

mod common {
    use super::*;

    #[derive(Debug)]
    pub struct Payload<'env> {
        pub video_path: &'env SimplePath,
        pub hashes: Vec<(Timestamp, Hamming, Mirror)>,
    }
}

mod video {
    use super::*;

    #[derive(Clone, Copy)]
    pub struct Ctx<'env> {
        pub preproc_args: &'env PreprocArgs,
        pub simi_args: &'env SimiArgs,
        pub ignored_hashes: &'env Ignored,
        pub new_files: &'env WorkQueue<&'env SimplePath>,
        pub repo_grave: Option<&'env Mutex<Repo>>,
    }

    pub fn main<'env>(
        ctx: Ctx<'env>,
        tx: mpsc::SyncSender<Payload<'env>>,
    ) -> eyre::Result<()> {
        log::debug!("video worker working");

        let mut failed = Vec::new();

        while let Some((i, vid_path)) = ctx.new_files.next_index() {
            log::info!("Progress: {}/{} videos", i + 1, ctx.new_files.len());
            let hashes = match get_hashes(ctx, vid_path) {
                Ok(ok) => ok,
                Err(e) => {
                    log::error!("Failed to get the hashes from '{}': {:?}", vid_path, e);
                    failed.push((vid_path, e));
                    continue;
                }
            };

            let load = Payload {
                video_path: vid_path,
                hashes,
            };
            if !try_send(&tx, load) {
                log::error!("The tree thread seems to be down");
                break;
            }
        }

        log::debug!("video worker ended");

        if !failed.is_empty() {
            let mut lines = vec!["Summary of videos that errored:".to_string()];
            lines.extend(
                failed
                    .into_iter()
                    .map(|(path, error)| format!("'{path}': {error:?}")),
            );
            eyre::bail!(lines.join("\n"));
        }

        Ok(())
    }

    fn get_hashes<'env>(
        ctx: Ctx<'env>,
        video: &'env SimplePath,
    ) -> eyre::Result<Vec<(Timestamp, Hamming, Mirror)>> {
        log::info!("Retrieving hashes for: {}", video);

        let mut extractor = FrameExtractor::new(video.as_path())
            .wrap_err("Failed to create the extractor")?;
        let approx_len = extractor.approx_length();

        // TODO: move to some config struct and add to Ctx
        let min_frames: NonZeroU32 = NonZeroU32::new(5).unwrap();
        let max_step: Duration = Duration::from_secs(10);
        let log_every = Duration::from_secs(10);

        let step = calc_step(approx_len, min_frames, max_step);
        // log::debug!("Stepping with {}s", step.as_secs_f64());

        let mut graveyard_entry = LazyEntry::new();

        let mut hashes = Vec::with_capacity(estimated_num_of_frames(approx_len, step));
        let approx_len = Timestamp::duration_to_string(approx_len);

        let mut last_logged = Instant::now();
        while let Some((ts, frame)) =
            extractor.next().wrap_err("Failed to get a frame")?
        {
            let now = Instant::now();
            if now - last_logged >= log_every {
                last_logged = now;
                log::debug!("At timestamp: {}/{}", ts.to_string(), approx_len);
            }

            use FrameToHashResult as F;
            match frame_to_hash(ctx, &frame, hashes.last().map(|(_, h, _)| *h)) {
                F::Ok(hash) => {
                    hashes.push((ts.clone(), hash, Mirror::Normal));
                    let mirror = imgutils::mirror(frame);
                    if let F::Ok(hash) = frame_to_hash(ctx, &mirror, Some(hash)) {
                        hashes.push((ts, hash, Mirror::Mirrored));
                    }
                }
                err @ F::Ignored
                | err @ F::Empty
                | err @ F::TooBlack
                | err @ F::TooBland
                    if ctx.repo_grave.is_some() =>
                {
                    let entry =
                        graveyard_entry.get_or_try_init(|| -> eyre::Result<_> {
                            let mut entry =
                                ctx.repo_grave.unwrap().lock().unwrap().new_entry()?;
                            entry.create_link_relative("original", video)?;
                            Ok(entry)
                        })?;

                    entry.create_jpg(
                        format!("{}_{}.jpg", err.name(), ts.to_string()),
                        &frame,
                    )?;
                }
                F::TooBlack
                | F::TooBland
                | F::TooSimilarToPrevious
                | F::Ignored
                | F::Empty => (),
            }

            extractor.seek_forward(step).wrap_err("Failed to seek")?;
        }

        log::info!("Got {} hashes from: {}", hashes.len(), video);
        Ok(hashes)
    }

    enum FrameToHashResult {
        Empty,
        TooBlack,
        TooBland,
        Ignored,
        TooSimilarToPrevious,
        Ok(Hamming),
    }

    impl FrameToHashResult {
        fn name(&self) -> &'static str {
            match self {
                FrameToHashResult::Empty => "empty",
                FrameToHashResult::TooBlack => "too_black",
                FrameToHashResult::TooBland => "too_bland",
                FrameToHashResult::Ignored => "ignored",
                FrameToHashResult::TooSimilarToPrevious => "similar_previous",
                FrameToHashResult::Ok(_) => "ok",
            }
        }
    }

    fn frame_to_hash<'env>(
        ctx: Ctx<'env>,
        frame: &RgbImage,
        last_hash: Option<Hamming>,
    ) -> FrameToHashResult {
        let hash = match ctx.preproc_args.hash_img(&frame) {
            Ok(hash) => hash,
            Err(PreprocError::Empty) => return FrameToHashResult::Empty,
            Err(PreprocError::TooBlack) => return FrameToHashResult::TooBlack,
            Err(PreprocError::TooBland) => return FrameToHashResult::TooBland,
        };

        if ctx.ignored_hashes.is_ignored(ctx.simi_args, hash) {
            return FrameToHashResult::Ignored;
        }

        match last_hash {
            None => FrameToHashResult::Ok(hash),
            Some(last_hash) if ctx.simi_args.are_dissimilar(hash, last_hash) => {
                FrameToHashResult::Ok(hash)
            }
            Some(_) => FrameToHashResult::TooSimilarToPrevious,
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

    fn try_send<'env>(tx: &mpsc::SyncSender<Payload<'env>>, load: Payload<'env>) -> bool {
        if let Err(load) = match tx.try_send(load) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(load)) => {
                log::warn!("The channel is full");
                Err(load)
            }
            Err(mpsc::TrySendError::Disconnected(load)) => Err(load),
        } {
            if let Err(_) = tx.send(load) {
                return false;
            }
        }
        true
    }
}

mod tree {

    use super::*;

    #[derive(Clone, Copy)]
    pub struct Ctx<'env> {
        pub simi_args: &'env SimiArgs,
    }

    // TODO: handle ctrl+c and properly close the db
    pub fn main<'env>(
        ctx: Ctx<'env>,
        rx: mpsc::Receiver<Payload<'env>>,
        mut tree: BKTree<VidSrc>,
        mut repo: Repo,
    ) -> eyre::Result<()> {
        log::debug!("Tree worker working");

        // TODO: timea de olika stegen och kolla vilken som är långsammast
        while let Ok(Payload { video_path, hashes }) = rx.recv() {
            log::info!("Finding dups of: {}", video_path);
            let similar_videos = find_similar_videos(ctx, &hashes, &mut tree)
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
        video_path: &SimplePath,
    ) -> eyre::Result<()> {
        tree.add_all(hashes.into_iter().map(|(ts, hash, mirrored)| {
            (hash, VidSrc::new(ts, video_path.to_owned(), mirrored))
        }))
        .wrap_err("failed to add to the tree")?;
        Ok(())
    }

    fn link_dup(
        repo: &mut Repo,
        video_path: &SimplePath,
        similar_videos: HashSet<&SimplePath>,
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

    fn find_similar_videos<'env>(
        ctx: Ctx<'env>,
        frames: &[(Timestamp, Hamming, Mirror)],
        tree: &'env mut BKTree<VidSrc>,
    ) -> eyre::Result<HashSet<&'env SimplePath>> {
        let sims: eyre::Result<Vec<Vec<_>>> = frames
            .par_iter()
            .map(|(_, hash, _)| hash)
            .map(|hash| -> eyre::Result<Vec<_>> {
                let mut res = Vec::new();
                tree.find_within(*hash, ctx.simi_args.threshold(), |_, src| {
                    res.push(src.path());
                })?;
                Ok(res)
            })
            .collect();

        let sims = sims.wrap_err("failed to find similar videos")?;
        Ok(sims.into_iter().flatten().collect())
    }
}
