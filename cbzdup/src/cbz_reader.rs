use std::{
    fs::File,
    io::{self, BufReader, Cursor, Read},
    path::PathBuf,
};

use image::RgbImage;
use image::{io::Reader as ImageReader, ImageError};
use zip::{result::ZipError, ZipArchive};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[error("Error when '{context}', apparently: {kind}")]
pub struct Error {
    context: String,
    kind: ErrorKind,
}

#[derive(Debug, thiserror::Error)]
enum ErrorKind {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("zip: {0}")]
    Zip(#[from] ZipError),
    #[error("image: {0}")]
    Image(#[from] ImageError),
}

trait ErrContext<T> {
    fn context<S: ToString, F: FnOnce() -> S>(self, provider: F) -> Result<T>;
}

impl<T, E> ErrContext<T> for std::result::Result<T, E>
where
    E: Into<ErrorKind>,
{
    fn context<S: ToString, F: FnOnce() -> S>(self, provider: F) -> Result<T> {
        self.map_err(|e| Error {
            context: provider().to_string(),
            kind: e.into(),
        })
    }
}

pub struct CbzReader {
    path: PathBuf,
    archive: ZipArchive<BufReader<File>>,
    index: usize,
}

impl CbzReader {
    pub fn new<P: Into<PathBuf>>(path: P) -> Result<Self> {
        let path = path.into();
        let ctx = || path.display();
        let file = File::open(&path).context(ctx)?;
        let archive = ZipArchive::new(BufReader::new(file)).context(ctx)?;
        Ok(Self {
            path,
            archive,
            index: 0,
        })
    }

    pub fn next(&mut self) -> Result<Option<RgbImage>> {
        while self.index < self.archive.len() {
            let ctx = || self.path.display();
            let mut file = self
                .archive
                .by_index({
                    let i = self.index;
                    self.index += 1;
                    i
                })
                .context(ctx)?;

            if !file.is_file() {
                continue;
            }

            let name = file.name().to_string();
            let ctx = || format!("{} -> {}", ctx(), name);

            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).context(ctx)?;

            let image = ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()
                .context(ctx)?
                .decode()
                .context(ctx)?;

            return Ok(Some(image.to_rgb8()));
        }

        Ok(None)
    }
}
