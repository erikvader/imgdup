use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use clap::Parser;
use color_eyre::eyre::{self, Context};
use imgdup::{
    bin_common::{
        ignored_hashes::read_ignored,
        init::{init_eyre, init_logger},
    },
    bktree::{
        mmap::bktree::BKTree,
        source_types::{any_source::AnySource, video_source::VidSrc},
    },
    imghash::{
        preproc::{PreprocArgs, PreprocCli},
        similarity::{SimiArgs, SimiCli},
    },
};

#[derive(Parser, Debug)]
#[command()]
/// Edit an existing database
struct Cli {
    #[command(flatten)]
    preproc_args: PreprocCli,

    #[command(flatten)]
    simi_args: SimiCli,

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
    Purge { dir: PathBuf },
    List { file: PathBuf },
}

fn goal_parser(s: &str) -> Result<Goal, String> {
    let parts: Vec<_> = s.split(':').collect();
    match &parts[..] {
        &["stats"] => Ok(Goal::Stats),
        &["rebuild"] => Ok(Goal::Rebuild),
        &["list", arg1] => Ok(Goal::List { file: arg1.into() }),
        &["purge", arg1] => Ok(Goal::Purge { dir: arg1.into() }),
        _ => Err(format!("Failed to parse goal '{s}', unrecognized")),
    }
}

fn main() -> eyre::Result<()> {
    init_eyre()?;
    init_logger(None)?;
    let cli = Cli::parse();

    let preproc_args = cli.preproc_args.to_args();
    let simi_args = cli.simi_args.to_args();

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
            Goal::Purge { ref dir } => {
                // TODO: create a macro for this temporary downcasting
                let mut vid_tree = tree.downcast().wrap_err("failed to downcast")?;
                let res = goal_purge(&mut vid_tree, &dir, &preproc_args, &simi_args);
                tree = vid_tree.upcast();
                res
            }
            Goal::List { ref file } => {
                let vid_tree = tree.downcast().wrap_err("failed to downcast")?;
                let res = goal_list(&vid_tree, &file);
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

fn goal_purge(
    tree: &mut BKTree<VidSrc>,
    dir: &Path,
    preproc_args: &PreprocArgs,
    simi_args: &SimiArgs,
) -> eyre::Result<()> {
    log::info!("Reading hashes to ignore");
    let ignored = read_ignored(dir, preproc_args, simi_args)
        .wrap_err_with(|| format!("failed to read hashes from: {}", dir.display()))?;
    log::info!("Read {}", ignored.len());

    log::info!("Removing them from the tree");
    let mut count = 0;
    tree.remove_any_of(|hash, vidsrc| {
        let matched = ignored.iter().any(|ign| simi_args.are_similar(ign, hash));

        if matched {
            // TODO: also print the timestamp. Have a nice general Display for vidsrc?
            log::debug!("Removing a frame with source: {}", vidsrc.path());
            count += 1;
        }

        matched
    })
    .wrap_err("failed to remove stuff in tree")?;
    log::info!("Removed a total of {} frames", count);

    Ok(())
}

fn goal_list(tree: &BKTree<VidSrc>, file_path: &Path) -> eyre::Result<()> {
    log::info!("Reading and sorting all entries");
    let lines = {
        let mut lines = Vec::new();
        tree.for_each(|hash, vidsrc| {
            let vidsrc = vidsrc.path();
            lines.push(format!("Source={vidsrc}, Hash={hash}"));
        })?;
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
