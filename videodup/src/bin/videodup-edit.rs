use std::{
    collections::HashSet,
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use clap::Parser;
use color_eyre::eyre::{self, Context};
use imgdup_common::{
    bin_common::init::{init_eyre, init_logger},
    bktree::{bktree::BKTree, source_types::AnySource},
    utils::{repo::Repo, simple_path::SimplePathBuf},
};
use rand::seq::IteratorRandom;
use videodup::{debug_info, video_source::VidSrc};

#[derive(Parser, Debug)]
#[command()]
/// Edit an existing database
struct Cli {
    /// Path to the database to use
    #[arg(long, short = 'f')]
    database_file: PathBuf,

    // TODO: list the goals in a description
    /// Goals to execute
    #[arg(value_parser = goal_parser, required = true)]
    goals: Vec<Goal>,
}

#[derive(Debug, Clone)]
enum Goal {
    Stats,
    Rebuild,
    Purge { repo: PathBuf },
    List { file: PathBuf },
    RandomDelete { num: usize },
}

fn goal_parser(s: &str) -> Result<Goal, String> {
    let parts: Vec<_> = s.split(':').collect();
    match &parts[..] {
        &["stats"] => Ok(Goal::Stats),
        &["rebuild"] => Ok(Goal::Rebuild),
        &["list", file] => Ok(Goal::List { file: file.into() }),
        &["purge", repo] => Ok(Goal::Purge { repo: repo.into() }),
        &["randel", num] => Ok(Goal::RandomDelete {
            num: num
                .parse::<usize>()
                .map_err(|_| "Expected a number".to_string())?,
        }),
        _ => Err(format!("Failed to parse goal '{s}', unrecognized")),
    }
}

// TODO: much of this should be able to be in its own executable, like an imgdup-edit that
// can ONLY handle AnySource. Should the VidSrc goals from this file be added as some kind
// of plugin? How to share the goal structure?
fn main() -> eyre::Result<()> {
    init_eyre()?;
    init_logger(None)?;
    let cli = Cli::parse();

    let mut tree =
        BKTree::<AnySource>::from_file(&cli.database_file).wrap_err_with(|| {
            format!(
                "failed to open the database at: {}",
                cli.database_file.display()
            )
        })?;

    for goal in cli.goals {
        log::info!("Performing goal: {goal:?}");
        match goal {
            Goal::Stats => goal_stats(&tree),
            Goal::Rebuild => match goal_rebuild(&mut tree, &cli.database_file) {
                Ok(new_tree) => {
                    tree = new_tree;
                    Ok(())
                }
                Err(e) => Err(e),
            },
            Goal::Purge { ref repo } => {
                // TODO: create a macro for this temporary downcasting
                let mut vid_tree = tree.downcast().wrap_err("failed to downcast")?;
                let res = goal_purge(&mut vid_tree, &repo);
                tree = vid_tree.upcast();
                res
            }
            Goal::List { ref file } => {
                let vid_tree = tree.downcast().wrap_err("failed to downcast")?;
                let res = goal_list(&vid_tree, &file);
                tree = vid_tree.upcast();
                res
            }
            Goal::RandomDelete { num } => {
                let mut vid_tree = tree.downcast().wrap_err("failed to downcast")?;
                let res = goal_random_delete(&mut vid_tree, num);
                tree = vid_tree.upcast();
                res
            }
        }
        .wrap_err_with(|| format!("failed to perform goal '{goal:?}'"))?;
        log::info!("Done with goal: {goal:?}");
    }

    tree.close().wrap_err("Failed to close the tree")?;

    Ok(())
}

fn goal_stats(tree: &BKTree<AnySource>) -> eyre::Result<()> {
    let (alive, dead) = tree.count_nodes().wrap_err("failed to count nodes")?;
    let total = alive + dead;
    log::info!("Stats:");
    log::info!("  Alive nodes = {alive}");
    log::info!(
        "  Dead  nodes = {dead} ({:.2}%)",
        (dead as f64 / total as f64) * 100.0
    );
    log::info!("  Total nodes = {total}");
    Ok(())
}

fn goal_rebuild(
    tree: &mut BKTree<AnySource>,
    db_file: &Path,
) -> eyre::Result<BKTree<AnySource>> {
    let tmp_file = {
        let mut db_name = db_file
            .file_name()
            .ok_or_else(|| eyre::eyre!("doesnt have a filename"))?
            .to_os_string();
        db_name.push(".rebuild");
        db_file.with_file_name(db_name)
    };

    let new_tree = tree.rebuild_to(&tmp_file).wrap_err("failed to rebuild")?;

    std::fs::rename(tmp_file, db_file).wrap_err("failed to overwrite the original")?;

    Ok(new_tree)
}

fn goal_purge(tree: &mut BKTree<VidSrc>, dir: &Path) -> eyre::Result<()> {
    log::info!("Purging every video from the dup dir: {}", dir.display());
    let mut all_videos = HashSet::new();
    let repo = Repo::new(dir).wrap_err("failed to open the dir as a repo")?;
    for entry in repo.entries()? {
        let collisions = entry
            .read_file(debug_info::DEBUG_INFO_FILENAME, |buf| {
                debug_info::read_from(buf)
            })
            .wrap_err("failed to read the debug info file")?;
        all_videos.extend(collisions.all().into_iter().map(|path| path.to_owned()));
    }

    log::info!("Removing {} different videos", all_videos.len());

    let mut counter = 0;
    tree.remove_any_of(|_, vidsrc| {
        let rm = all_videos.contains(vidsrc.path());
        if rm {
            counter += 1;
        }
        rm
    })
    .wrap_err("failed to remove from the tree")?;

    log::info!("Removed {counter} nodes from the tree");

    Ok(())
}

fn goal_list(tree: &BKTree<VidSrc>, file_path: &Path) -> eyre::Result<()> {
    log::info!("Reading and sorting all entries");
    let lines = {
        let mut lines = Vec::new();
        tree.for_each(|hash, vidsrc| {
            let vidsrc = vidsrc.path();
            lines.push(format!("Source={vidsrc}, Hash={hash}"));
        })
        .wrap_err("failed to iterate through the tree")?;
        lines.sort();
        lines
    };

    log::info!("Writing them to a file at: {}", file_path.display());
    let mut file = BufWriter::new(
        File::create(file_path)
            .wrap_err_with(|| format!("failed to open file: {}", file_path.display()))?,
    );
    for line in lines {
        writeln!(file, "{line}").wrap_err("failed to call write")?;
    }
    file.flush().wrap_err("failed to flush")?;

    log::info!("Wrote the entries in the tree to a file");
    Ok(())
}

fn goal_random_delete(vid_tree: &mut BKTree<VidSrc>, num: usize) -> eyre::Result<()> {
    log::info!("Finding all video paths in the tree");
    let videos = {
        let mut videos = HashSet::new();
        vid_tree
            .for_each(|_, vidsrc| {
                videos.insert(vidsrc.path());
            })
            .wrap_err("failed to find all video paths")?;
        videos
    };
    log::info!("Found {} video paths", videos.len());

    let to_remove: HashSet<SimplePathBuf> = videos
        .into_iter()
        .choose_multiple(&mut rand::thread_rng(), num)
        .into_iter()
        .map(|p| p.to_owned())
        .inspect(|p| log::info!("Will remove '{p}'"))
        .collect();

    log::info!("Removing stuff...");
    vid_tree
        .remove_any_of(|_, vidsrc| to_remove.contains(vidsrc.path()))
        .wrap_err("failed to remove all nodes from the tree")?;
    log::info!("Done!");

    Ok(())
}
