use ffmpeg::format::Pixel;
use ffmpeg::software::scaling::context::Context as ScalingContext;
use ffmpeg::{codec::Context, format::input, frame::Video, media::Type};

extern crate ffmpeg_next as ffmpeg;

type Result<T> = std::result::Result<T, ffmpeg::Error>;

fn main() -> Result<()> {
    ffmpeg::init()?;
    let mut ictx = input(&"./video.mkv")?;
    let video = ictx
        .streams()
        .best(Type::Video)
        .ok_or(ffmpeg::Error::StreamNotFound)?;
    let video_stream_index = video.index();

    let context_decoder = Context::from_parameters(video.parameters())?;
    let mut decoder = context_decoder.decoder().video()?;

    let mut scaler = ScalingContext::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGB24,
        decoder.width(),
        decoder.height(),
        ffmpeg::software::scaling::Flags::BILINEAR,
    )?;

    let mut frame_index = 0;

    for (stream, packet) in ictx.packets() {
        if stream.index() != video_stream_index {
            continue;
        }
        decoder.send_packet(&packet)?;

        let mut raw = Video::empty();
        // TODO: check if AVERROR(EAGAIN) eller AVERROR_EOF för att avsluta loopen
        while decoder.receive_frame(&mut raw).is_ok() {
            let mut scaled = Video::empty();
            scaler.run(&raw, &mut scaled)?;
            println!("{}, {:?}", frame_index, scaled.data(0).get(0));
            frame_index += 1;
        }
    }
    decoder.send_eof()?;

    Ok(())
}
