extern crate ffmpeg_next as ffmpeg;

use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use ffmpeg::codec::Context as CodecContext;
use ffmpeg::decoder::Video as DecoderVideo;
use ffmpeg::format::context::Input as FormatContext;
use ffmpeg::format::Pixel;
use ffmpeg::frame::Video as FrameVideo;
use ffmpeg::software::scaling::context::Context as ScalingContext;
use ffmpeg::{format::input, media::Type};
use ffmpeg::{Packet as CodecPacket, Rational, Rescale};
use image::RgbImage;

#[derive(thiserror::Error, Debug)]
pub enum FrameExtractorError {
    #[error("ffmpeg: {0}")]
    Ffmpeg(#[from] ffmpeg::Error),
}

pub type Result<T> = std::result::Result<T, FrameExtractorError>;

static FFMPEG_INITIALIZED: OnceLock<std::result::Result<(), ffmpeg::Error>> =
    OnceLock::new();

pub enum FrameExtractor {
    Active {
        ictx: FormatContext,
        decoder: DecoderVideo,
        video_stream_index: usize,
        converter: ScalingContext,
        cur_timestamp: i64,
        end_timestamp: i64,
        target_timestamp: i64,
        timebase: Rational,
    },
    Done,
}

impl FrameExtractor {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        if let Err(e) = FFMPEG_INITIALIZED.get_or_init(|| ffmpeg::init()) {
            return Err(e.clone().into());
        }

        let ictx = input(&path)?;
        // TODO: somehow set the discard property on everything, except the video, to
        // improve seeking
        let video = ictx
            .streams()
            .best(Type::Video)
            .ok_or(ffmpeg::Error::StreamNotFound)?;
        let video_stream_index = video.index();
        let cur_timestamp = video.start_time(); // TODO: is this really correct?
        let end_timestamp = video.duration();
        let timebase = video.time_base();

        let decoder = CodecContext::from_parameters(video.parameters())?
            .decoder()
            .video()?;

        let converter = Self::pixel_converter(&decoder)?;

        Ok(Self::Active {
            ictx,
            decoder,
            video_stream_index,
            converter,
            cur_timestamp,
            end_timestamp,
            target_timestamp: cur_timestamp,
            timebase,
        })
    }

    fn pixel_converter(decoder: &DecoderVideo) -> Result<ScalingContext> {
        ScalingContext::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            // http://git.videolan.org/?p=ffmpeg.git;a=blob;f=libavutil/pixfmt.h;hb=HEAD
            Pixel::RGB24,
            decoder.width(),
            decoder.height(),
            ffmpeg::software::scaling::Flags::FAST_BILINEAR,
        )
        .map_err(|e| e.into())
    }

    fn set_done(&mut self) {
        *self = Self::Done;
    }

    pub fn next(&mut self) -> Result<Option<(Duration, RgbImage)>> {
        while let Self::Active {
            ictx,
            decoder,
            video_stream_index,
            converter,
            cur_timestamp,
            target_timestamp,
            timebase,
            ..
        } = self
        {
            // http://ffmpeg.org/doxygen/trunk/group__lavf__decoding.html#ga4fdb3084415a82e3810de6ee60e46a61
            let mut packet = CodecPacket::empty();
            match packet.read(ictx) {
                Ok(()) => (),
                Err(ffmpeg::Error::Eof) => {
                    self.set_done();
                    break;
                }
                Err(e) => return Err(e.into()),
            }

            if packet.stream() != *video_stream_index {
                continue;
            }

            decoder.send_packet(&packet)?;

            loop {
                let mut frame = FrameVideo::empty();
                // avcodec_receive_frame
                // https://ffmpeg.org/doxygen/trunk/group__lavc__decoding.html#ga11e6542c4e66d3028668788a1a74217c
                match decoder.receive_frame(&mut frame) {
                    Ok(()) => (),
                    Err(ffmpeg::Error::Other {
                        errno: libc::EAGAIN,
                    }) => break,
                    Err(ffmpeg::Error::Eof) => {
                        self.set_done();
                        break;
                    }
                    Err(e) => return Err(e.into()),
                }

                *cur_timestamp = frame
                    .timestamp()
                    .expect("this is always set by the decoder?");

                if *cur_timestamp < *target_timestamp {
                    continue;
                }

                let mut converted = FrameVideo::empty();
                converter.run(&frame, &mut converted)?;
                assert_eq!(1, converted.planes());
                let img = RgbImage::from_vec(
                    converted.width(),
                    converted.height(),
                    converted.data(0).to_vec(),
                )
                .expect("the buffer is big enough!");

                let dur = timestamp2duration(*cur_timestamp, *timebase);
                return Ok(Some((dur, img)));
            }
        }

        Ok(None)
    }

    pub fn seek_forward(&mut self, dur: Duration) -> Result<()> {
        match self {
            Self::Active {
                ictx,
                cur_timestamp,
                video_stream_index,
                timebase,
                decoder,
                target_timestamp,
                ..
            } => {
                let target = *cur_timestamp + duration2timestamp(dur, *timebase);
                seek(ictx, *video_stream_index, target, ..)?;
                decoder.flush();
                *target_timestamp = target;
                Ok(())
            }
            Self::Done => panic!("can't seek when done"),
        }
    }
}

// TODO: Does this need to take start_time into account?
fn duration2timestamp(dur: Duration, timebase: Rational) -> i64 {
    let step: i64 = dur
        .as_millis()
        .try_into()
        .expect("will probably not be that big");
    let to_seconds = Rational::new(1, 1000);
    // NOTE: This is: step * to_seconds / timebase
    step.rescale(to_seconds, timebase)
}

// TODO: Does this need to take start_time into account?
fn timestamp2duration(timestamp: i64, timebase: Rational) -> Duration {
    let to_seconds = Rational::new(1, 1000);
    // NOTE: timestamp * timebase / to_seconds
    let millis = timestamp.rescale(timebase, to_seconds);
    Duration::from_millis(millis.try_into().expect("probably not a problem"))
}

/// A copy of FormatContext::seek, except that this accepts a stream_index to seek on.
fn seek<R: ffmpeg::util::range::Range<i64>>(
    input: &mut FormatContext,
    stream_index: usize,
    ts: i64,
    range: R,
) -> std::result::Result<(), ffmpeg::Error> {
    unsafe {
        match ffmpeg_sys_next::avformat_seek_file(
            input.as_mut_ptr(),
            stream_index
                .try_into()
                .expect("will probably not be that big"),
            range.start().cloned().unwrap_or(i64::min_value()),
            ts,
            range.end().cloned().unwrap_or(i64::max_value()),
            0,
        ) {
            s if s >= 0 => Ok(()),
            e => Err(ffmpeg::Error::from(e)),
        }
    }
}
