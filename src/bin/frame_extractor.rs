use std::time::Duration;

use imgdup::frame_extractor::FrameExtractor;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        eprintln!("invalid usage");
        eprintln!("{} videofile step outdir num_frames", args[0]);
        anyhow::bail!("invalid usage");
    }

    let filename = &args[1];
    let step: u64 = args[2].parse()?;
    let step = Duration::from_secs(step);
    let outdir = &args[3];
    let num_frames: usize = args[4].parse()?;

    let mut extractor = FrameExtractor::new(filename)?;
    for i in 1..=num_frames {
        match extractor.next() {
            Ok(Some((ts, img))) => {
                let frame_filename =
                    format!("{}/frame_{}_{}.jpg", outdir, i, ts.to_string());
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
