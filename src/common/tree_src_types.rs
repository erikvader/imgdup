use std::{
    fmt,
    path::{Path, PathBuf},
};

use crate::{frame_extractor::timestamp::Timestamp, fsutils::is_simple_relative};

// TODO: https://docs.rs/quote/1.0.33/quote/macro.quote.html
// TODO: https://docs.rs/proc-macro2/1.0.66/proc_macro2/struct.TokenStream.html#impl-Display-for-TokenStream
#[derive(serde::Serialize, serde::Deserialize, Clone, Hash, PartialEq, Eq)]
pub struct VidSrc {
    frame_pos: Timestamp,
    path: PathBuf,
    mirrored: Mirror,
}

#[derive(
    serde::Serialize, serde::Deserialize, Copy, Clone, Hash, PartialEq, Eq, Debug,
)]
pub enum Mirror {
    Normal,
    Mirrored,
}

impl fmt::Display for Mirror {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self, f)
    }
}

impl fmt::Display for VidSrc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?}:{}:{}",
            self.path,
            self.frame_pos,
            match self.mirrored {
                Mirror::Normal => "N",
                Mirror::Mirrored => "M",
            },
        )
    }
}

impl VidSrc {
    pub fn new(frame_pos: Timestamp, path: PathBuf, mirrored: Mirror) -> Self {
        assert!(is_simple_relative(&path));
        Self {
            frame_pos,
            path,
            mirrored,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn frame_pos(&self) -> &Timestamp {
        &self.frame_pos
    }

    pub fn mirrored(&self) -> Mirror {
        self.mirrored
    }
}
