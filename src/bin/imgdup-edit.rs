use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use clap::Parser;
use color_eyre::eyre::{self, Context};
use imgdup::{
    bktree::BKTree,
    common::{
        hash_images::{read_ignored, HashCli, HashConf},
        init_eyre, init_logger,
        tree_src_types::VidSrc,
    },
};

#[derive(Parser, Debug)]
#[command()]
/// Edit an existing database
struct Cli {
    /// Path to the database to use
    #[arg(long, short = 'f')]
    database_file: PathBuf,

    #[command(flatten)]
    hash_args: HashCli,

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

    let mut tree =
        BKTree::<VidSrc>::from_file(&cli.database_file).wrap_err_with(|| {
            format!(
                "failed to open the database at: {}",
                cli.database_file.display()
            )
        })?;

    let hash_conf = cli.hash_args.as_conf();

    for goal in cli.goals {
        log::info!("Performing goal: {goal:?}");
        match goal {
            Goal::Stats => goal_stats(&mut tree),
            Goal::Rebuild => goal_rebuild(&mut tree),
            Goal::Purge { ref dir } => goal_purge(&mut tree, &dir, &hash_conf),
            Goal::List { ref file } => goal_list(&mut tree, &file),
        }
        .wrap_err_with(|| format!("failed to perform goal '{goal:?}'"))?;
        log::info!("Done with goal: {goal:?}");
    }

    tree.close().wrap_err("Failed to close the tree")?;

    Ok(())
}

fn goal_stats(tree: &mut BKTree<VidSrc>) -> eyre::Result<()> {
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

fn goal_rebuild(tree: &mut BKTree<VidSrc>) -> eyre::Result<()> {
    let (alive, dead) = tree.rebuild().wrap_err("failed to rebuild")?;
    let before = alive + dead;
    let after = alive;
    log::info!("Stats after rebuild:");
    log::info!("  Nodes before = {before}");
    log::info!("  Nodes  after = {after}");
    Ok(())
}

fn goal_purge(
    tree: &mut BKTree<VidSrc>,
    dir: &Path,
    hash_conf: &HashConf,
) -> eyre::Result<()> {
    log::info!("Reading hashes to ignore");
    let ignored = read_ignored(dir, hash_conf)
        .wrap_err_with(|| format!("failed to read hashes from: {}", dir.display()))?;
    log::info!("Read {}", ignored.len());

    log::info!("Removing them from the tree");
    let mut count = 0;
    tree.remove_any_of(|hash, vidsrc| {
        let matched = ignored
            .iter()
            .any(|ign| ign.distance_to(hash) <= hash_conf.similarity_threshold);

        if matched {
            log::debug!("Removing a frame with source: {}", vidsrc);
            count += 1;
        }

        matched
    })
    .wrap_err("failed to remove stuff in tree")?;
    log::info!("Removed a total of {} frames", count);

    Ok(())
}

fn goal_list(tree: &mut BKTree<VidSrc>, file_path: &Path) -> eyre::Result<()> {
    log::info!("Reading and sorting all entries");
    let lines = {
        let mut lines = Vec::new();
        tree.for_each(|hash, vidsrc| {
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