use std::time::Duration;

use imgdup::frame_extractor::FrameExtractor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("invalid usage");
        return Ok(());
    }

    let filename = &args[1];
    let step: u64 = args[2].parse().unwrap();
    let step = Duration::from_secs(step);

    let mut extractor = FrameExtractor::new(filename)?;
    for i in 1..=10 {
        match extractor.next() {
            Ok(Some((ts, img))) => {
                let frame_filename = format!("frames/frame_{}_{}.jpg", i, ts.to_string());
                println!("Writing {}", frame_filename);
                img.save(frame_filename)?;
            }
            Ok(None) => break,
            Err(e) => return Err(e.into()),
        }
        extractor.seek_forward(step)?;
    }

    Ok(())
}
