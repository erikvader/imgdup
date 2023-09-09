use std::path::{Path, PathBuf};

use crate::{frame_extractor::timestamp::Timestamp, fsutils::is_simple_relative};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct VidSrc {
    frame_pos: Timestamp,
    // TODO: figure out a way to not store the whole path for every single hash
    path: PathBuf,
}

impl VidSrc {
    pub fn new(frame_pos: Timestamp, path: PathBuf) -> Self {
        assert!(is_simple_relative(&path));
        Self { frame_pos, path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn frame_pos(&self) -> &Timestamp {
        &self.frame_pos
    }
}
