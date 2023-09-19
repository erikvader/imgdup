use std::path::{Path, PathBuf};

use clap::Parser;
use color_eyre::eyre::{self, Context};
use imgdup::{
    bktree::BKTree,
    common::{
        hash_images::{HashCli, HashConf},
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
}

fn goal_parser(s: &str) -> Result<Goal, String> {
    let parts: Vec<_> = s.split(':').collect();
    match &parts[..] {
        &["stats"] => Ok(Goal::Stats),
        &["rebuild"] => Ok(Goal::Rebuild),
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
    log::info!("  Dead  nodes = {dead}");
    log::info!("  Total nodes = {total}");
    Ok(())
}

fn goal_rebuild(tree: &mut BKTree<VidSrc>) -> eyre::Result<()> {
    let (alive, dead) = tree.rebuild().wrap_err("failed to rebuild")?;
    let total = alive + dead;
    log::info!("Stats after rebuild:");
    log::info!("  Alive nodes = {alive}");
    log::info!("  Dead  nodes = {dead}");
    log::info!("  Total nodes = {total}");
    Ok(())
}

fn goal_purge(
    tree: &mut BKTree<VidSrc>,
    dir: &Path,
    hash_conf: &HashConf,
) -> eyre::Result<()> {
    // TODO: read all hashes from `dir` and remove them from `tree`
    todo!()
    Ok(())
}
