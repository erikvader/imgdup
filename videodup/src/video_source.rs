use std::fmt;

use rkyv::{Archive, Serialize};

use crate::frame_extractor::timestamp::{ArchivedTimestamp, Timestamp};
use imgdup_common::{
    bktree::source_types::Source,
    utils::simple_path::{SimplePath, SimplePathBuf},
};

#[derive(
    Serialize, Archive, Clone, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize,
)]
#[archive(check_bytes)]
pub struct VidSrc {
    frame_pos: Timestamp,
    path: SimplePathBuf,
    mirrored: Mirror,
}

#[derive(
    Serialize,
    Archive,
    Copy,
    Clone,
    Hash,
    PartialEq,
    Eq,
    Debug,
    serde::Serialize,
    serde::Deserialize,
)]
#[archive(check_bytes)]
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
    pub fn new(frame_pos: Timestamp, path: SimplePathBuf, mirrored: Mirror) -> Self {
        Self {
            frame_pos,
            path,
            mirrored,
        }
    }

    pub fn path(&self) -> &SimplePath {
        &self.path
    }

    pub fn frame_pos(&self) -> &Timestamp {
        &self.frame_pos
    }

    pub fn mirrored(&self) -> Mirror {
        self.mirrored
    }
}

impl ArchivedVidSrc {
    pub fn path(&self) -> &SimplePath {
        self.path.as_ref()
    }

    pub fn frame_pos(&self) -> &ArchivedTimestamp {
        &self.frame_pos
    }

    pub fn mirrored(&self) -> Mirror {
        match self.mirrored {
            ArchivedMirror::Normal => Mirror::Normal,
            ArchivedMirror::Mirrored => Mirror::Mirrored,
        }
    }

    // TODO: figure out of rkyv deserialize works and use that instead
    pub fn deserialize(&self) -> VidSrc {
        VidSrc {
            frame_pos: self.frame_pos.deserialize(),
            path: self.path.deserialize(),
            mirrored: self.mirrored(),
        }
    }
}

impl Source for VidSrc {
    fn identifier() -> &'static str {
        "video:1"
    }
}
