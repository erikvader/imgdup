mod common;

use common::cargo_tmpdir;
use imgdup::frame_extractor::{self, FrameExtractor};
use std::{path::PathBuf, process::Stdio, time::Duration};

const TEST_VIDEO_FRAMES: usize = 250;
const TEST_VIDEO_LENGTH_SEC: u64 = 10;

#[test]
fn test_seeking_at_eof() -> frame_extractor::Result<()> {
    let video = create_test_video();
    let mut frames = FrameExtractor::new(video)?;

    frames.seek_forward(Duration::from_secs(TEST_VIDEO_LENGTH_SEC - 1))?;
    assert!(frames.iter().all(|frame| frame.is_ok()));

    frames.seek_forward(Duration::from_secs(1))?;
    assert!(matches!(frames.next(), Ok(None)));
    Ok(())
}

#[test]
fn test_total_frames() -> frame_extractor::Result<()> {
    let video = create_test_video();
    let num_frames = FrameExtractor::new(video)?
        .iter()
        .filter(|res| res.is_ok())
        .take(TEST_VIDEO_FRAMES + 1)
        .count();
    assert_eq!(TEST_VIDEO_FRAMES, num_frames);
    Ok(())
}

fn create_test_video() -> PathBuf {
    let tmpvideo = cargo_tmpdir().join("testvideo.mkv");

    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::fs::remove_file(&tmpvideo).ok();
        std::process::Command::new("ffmpeg")
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=10:rate=25",
                tmpvideo.as_os_str().to_str().expect("no probs, probably"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .status()
            .expect("failed to execute ffmpeg");
    });

    tmpvideo
}
