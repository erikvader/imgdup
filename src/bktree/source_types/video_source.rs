use std::{fmt, path::Path};

use rkyv::{Archive, Serialize};

use crate::frame_extractor::timestamp::ArchivedTimestamp;
use crate::utils::simple_path::SimplePath;
use crate::{frame_extractor::timestamp::Timestamp, utils::simple_path::SimplePathBuf};

#[derive(Serialize, Archive, Clone, Hash, PartialEq, Eq)]
#[archive(check_bytes)]
pub struct VidSrc {
    frame_pos: Timestamp,
    path: SimplePathBuf,
    mirrored: Mirror,
}

#[derive(Serialize, Archive, Copy, Clone, Hash, PartialEq, Eq, Debug)]
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
}

impl super::private::Seal for VidSrc {}
impl super::PartialSource for VidSrc {
    fn identifier() -> Option<&'static str> {
        Some("video:1")
    }
}
impl super::Source for VidSrc {}
