use std::{path::PathBuf, process::Stdio};

#[test]
fn test_seeking_at_eof() {
    let video = create_test_video();
    // sök till slutet
    // hämta frames tills det inte går längre
    // sök framåt
    // kolla att det fortfarande är Ok(None)
}

fn create_test_video() -> PathBuf {
    let tmpdir =
        PathBuf::from(option_env!("CARGO_TARGET_TMPDIR").unwrap()).join("testvideo.mkv");

    if tmpdir.exists() {
        return tmpdir;
    }

    std::process::Command::new("ffmpeg")
        .args([
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=10:size=1280x720:rate=30",
            tmpdir.as_os_str().to_str().expect("no probs, probably"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .status()
        .expect("failed to execute ffmpeg");

    tmpdir
}
