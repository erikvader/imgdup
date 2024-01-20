use std::collections::HashSet;

use imgdup_common::{imghash::hamming::Hamming, utils::simple_path::SimplePath};

use crate::video_source::VidSrc;

pub const DEBUG_INFO_FILENAME: &str = "debuginfo";

/// A video frame with its hash and where to find it
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Frame {
    pub hash: Hamming,
    pub vidsrc: VidSrc,
}

/// A collision between two frames, they are considered very similar
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Collision {
    pub reference: Frame,
    pub other: Frame,
}

/// A whole bunch of collisions. All reference frames generally come from the same file.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Collisions {
    pub collisions: Vec<Collision>,
}

impl Collisions {
    pub fn all_others(&self) -> HashSet<&SimplePath> {
        self.collisions
            .iter()
            .map(|col| col.other.vidsrc.path())
            .collect()
    }

    pub fn all(&self) -> HashSet<&SimplePath> {
        self.collisions
            .iter()
            .flat_map(|col| [col.reference.vidsrc.path(), col.other.vidsrc.path()])
            .collect()
    }
}

pub fn save_to(writer: impl std::io::Write, info: &Collisions) -> ron::Result<()> {
    let conf = ron::ser::PrettyConfig::new().struct_names(true);
    ron::ser::to_writer_pretty(writer, info, conf)
}

pub fn read_from(reader: impl std::io::Read) -> ron::error::SpannedResult<Collisions> {
    ron::de::from_reader(reader)
}
