use color_eyre::eyre::{self, Context};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
};

use clap::Parser;
use imgdup::{
    imghash::{
        self,
        hamming::{Distance, Hamming},
    },
    imgutils::{self, RemoveBordersConf},
    plot,
};

#[derive(Parser)]
#[command()]
/// Hash pictures and compare them against each other
struct Cli {
    /// All gray values below this becomes black
    #[arg(long, short = 't', default_value_t = imgutils::DEFAULT_MASKIFY_THRESHOLD)]
    maskify_threshold: u8,

    /// A mask line can contain this many percent of white and still be considered black
    #[arg(long, short = 'w', default_value_t = imgutils::DEFAULT_BORDER_MAX_WHITES)]
    maximum_whites: f64,

    /// Save all collisions below this distance
    #[arg(long, short = 'c')]
    save_collisions: Option<Distance>,

    /// Folders with pictures in them
    #[arg(required = true)]
    picture_folders: Vec<PathBuf>,
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    println!("Finding all filenames...");
    let pictures = all_files(&cli.picture_folders)?;

    println!("Hashing all pictures...");
    let hashes = hash_pictures(
        &pictures,
        RemoveBordersConf::default()
            .with_maskify_threshold(cli.maskify_threshold)
            .with_maximum_whites(cli.maximum_whites),
    )?;

    assert_eq!(hashes.len(), pictures.len());

    println!("Comparing all distances...");
    let pairwise = compare_all(&hashes);
    let distances = count_distances(&pairwise);

    println!("Writing text files...");
    write_text_files(&pairwise, &pictures)?;

    println!("Writing the graph...");
    write_graph_file(&distances)?;

    if let Some(max_dist) = cli.save_collisions {
        println!("Creating collision symlinks...");
        point_collisions(&pictures, &pairwise, max_dist)?;
    }

    Ok(())
}

fn all_files(folders: &[PathBuf]) -> eyre::Result<Vec<PathBuf>> {
    let mut files = vec![];
    for dir in folders {
        for pic in fs::read_dir(&dir)
            .wrap_err_with(|| format!("Failed to read contents of: {dir:?}"))?
        {
            let pic =
                pic.wrap_err_with(|| format!("Failed to read an entry of: {dir:?}"))?;
            files.push(pic.path());
        }
    }
    Ok(files)
}

pub fn symlink(point: impl AsRef<Path>, filename: impl AsRef<Path>) -> io::Result<()> {
    let point = match point.as_ref() {
        p if p.is_relative() => std::env::current_dir()?.join(p),
        p => p.to_path_buf(),
    };

    let filename = match filename.as_ref() {
        f if f.is_dir() => f.join(point.file_name().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "point path does not refer to anything",
            )
        })?),
        f => f.to_path_buf(),
    };

    std::os::unix::fs::symlink(point, filename)
}

pub fn clear_dir(dir: impl AsRef<Path>) -> io::Result<()> {
    let dir = dir.as_ref();
    match fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() => {
            fs::remove_dir_all(dir)?;
            fs::create_dir(dir)
        }
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "dir is not a dir",
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => fs::create_dir(dir),
        Err(e) => Err(e),
    }
}

pub fn path_as_filename(p: impl AsRef<Path>) -> String {
    p.as_ref().display().to_string().replace('/', "!")
}

fn hash_pictures(
    pictures: &[PathBuf],
    config: RemoveBordersConf,
) -> image::ImageResult<Vec<Option<Hamming>>> {
    let mut file = BufWriter::new(File::create("empty.txt")?);
    let empty_dir = Path::new("empty");
    clear_dir(&empty_dir)?;

    let mut hashes = vec![];
    let total = pictures.len();
    for (i, pic_path) in pictures.iter().enumerate() {
        println!("Hash: {}/{}", i + 1, total);
        let img = image::open(pic_path)?.to_rgb8();
        let cropped = imgutils::remove_borders(&img, &config).to_image();

        let h = if imgutils::img_empty(&cropped) {
            println!("Empty: {pic_path:?}");
            writeln!(&mut file, "{:?}", pic_path.display())?;
            symlink(pic_path, empty_dir).ok();
            None
        } else {
            Some(imghash::hash(&cropped))
        };

        hashes.push(h);
    }

    file.flush()?;

    Ok(hashes)
}

fn compare_all(hashes: &[Option<Hamming>]) -> Vec<(usize, usize, Distance)> {
    let mut dists = Vec::with_capacity(hashes.len() * (hashes.len() + 1) / 2);
    for (i, h1) in hashes.iter().enumerate() {
        if h1.is_none() {
            continue;
        }

        for (j, h2) in hashes[i + 1..].iter().enumerate() {
            if h2.is_none() {
                continue;
            }

            let d = h1.unwrap().distance_to(h2.unwrap());
            dists.push((i, j + i + 1, d));
        }
    }
    dists
}

fn count_distances(pairwise: &[(usize, usize, Distance)]) -> HashMap<Distance, u32> {
    let mut dists = HashMap::new();
    for d in Hamming::MIN_DIST..=Hamming::MAX_DIST {
        dists.insert(d, 0);
    }
    pairwise
        .iter()
        .for_each(|(_, _, d)| *dists.entry(*d).or_default() += 1);
    dists
}

fn write_text_files(
    pairwise: &[(usize, usize, Distance)],
    pictures: &[PathBuf],
) -> io::Result<()> {
    let mut file = BufWriter::new(File::create("pictures.txt")?);
    for (i, pic) in pictures.iter().enumerate() {
        writeln!(&mut file, "{:04}: {}", i, pic.display())?;
    }
    file.flush()?;

    let mut file = BufWriter::new(File::create("distances.txt")?);
    for (i, j, dist) in pairwise {
        writeln!(&mut file, "{i:04}-{j:04}={dist}")?;
    }
    file.flush()?;

    Ok(())
}

fn write_graph_file(distances: &HashMap<Distance, u32>) -> eyre::Result<()> {
    let mut bars: Vec<(Distance, u32)> = distances
        .iter()
        .map(|(dist, count)| (*dist, *count))
        .collect();
    bars.sort_unstable();

    plot::bar_chart("distances.svg", &bars)
}

fn point_collisions(
    pictures: &[PathBuf],
    pairwise: &[(usize, usize, Distance)],
    max_dist: Distance,
) -> eyre::Result<()> {
    let col_dir = Path::new("collisions");
    clear_dir(&col_dir)?;

    fn linkit(pic: &PathBuf, dir: &Path) -> eyre::Result<()> {
        let target = dir.join(path_as_filename(pic));
        symlink(pic, &target).wrap_err_with(|| {
            format!(
                "Could not create symlink to {} at {}",
                pic.display(),
                target.display()
            )
        })
    }

    for (i, (p1, p2, dist)) in pairwise.iter().enumerate() {
        if *dist > max_dist {
            continue;
        }

        let dir = col_dir.join(i.to_string());
        fs::create_dir(&dir).wrap_err_with(|| format!("Could not create dir {i}"))?;

        linkit(&pictures[*p1], &dir)?;
        linkit(&pictures[*p2], &dir)?;
    }

    Ok(())
}
