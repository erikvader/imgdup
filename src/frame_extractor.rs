extern crate ffmpeg_next as ffmpeg;

pub mod timestamp;

use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use ffmpeg::codec::Context as CodecContext;
use ffmpeg::decoder::Video as DecoderVideo;
use ffmpeg::format::context::Input as FormatContext;
use ffmpeg::format::Pixel;
use ffmpeg::frame::Video as FrameVideo;
use ffmpeg::software::scaling::context::Context as ScalingContext;
use ffmpeg::util::log as ffmpeglog;
use ffmpeg::{format::input, media::Type};
use ffmpeg::{Packet as CodecPacket, Rational, Rescale};
use image::RgbImage;

use self::timestamp::Timestamp;

#[derive(thiserror::Error, Debug)]
pub enum FrameExtractorError {
    #[error("ffmpeg: {0}")]
    Ffmpeg(#[from] ffmpeg::Error),
    #[error("timestamp does not seem to originate from the same file")]
    TimestampMismatch,
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

        seek_target_timestamp: i64,
        cur_timestamp: i64,

        end_timestamp: i64,
        first_timestamp: i64,
        timebase: Rational,
    },
    Done,
}

impl FrameExtractor {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        if let Err(e) = FFMPEG_INITIALIZED.get_or_init(|| {
            ffmpeg::init()?;
            // TODO: Save more logs from maybe level error or warning.
            // https://www.ffmpeg.org/doxygen/trunk/group__lavu__log__constants.html#ga11e329935b59b83ca722b66674f37fd4
            // Somehow set a callback with av_log_set_callback.
            // https://github.com/zmwangx/rust-ffmpeg/pull/91
            // Give back a list of error messages in the `next` function alongside the
            // picture?
            ffmpeglog::set_level(ffmpeglog::Level::Fatal);
            Ok(())
        }) {
            return Err(e.clone().into());
        }

        let ictx = input(&path)?;
        // TODO: somehow set the discard property on everything, except the video, to
        // improve seeking. There doesn't seem to be a way to do this in ffmpeg-next 6.0.0
        let video = ictx
            .streams()
            .best(Type::Video)
            .ok_or(ffmpeg::Error::StreamNotFound)?;
        let video_stream_index = video.index();
        let cur_timestamp = video.start_time();
        let seek_target_timestamp = video.start_time();
        let first_timestamp = video.start_time();
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
            seek_target_timestamp,
            first_timestamp,
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

    pub fn next(&mut self) -> Result<Option<(Timestamp, RgbImage)>> {
        while let Self::Active {
            ictx,
            decoder,
            video_stream_index,
            converter,
            cur_timestamp,
            seek_target_timestamp,
            timebase,
            first_timestamp,
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

                if *cur_timestamp < *seek_target_timestamp {
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

                let dur = Timestamp::new(*cur_timestamp, *timebase, *first_timestamp);
                return Ok(Some((dur, img)));
            }
        }

        Ok(None)
    }

    pub fn seek_forward(&mut self, dur: Duration) -> Result<()> {
        match self {
            Self::Active {
                cur_timestamp,
                timebase,
                ..
            } => {
                if dur.is_zero() {
                    return Ok(());
                }
                let target = *cur_timestamp + duration2timestamp(dur, *timebase);
                self.seek_internal(target)
            }
            Self::Done => panic!("can't seek when done"),
        }
    }

    pub fn seek_to(&mut self, timestamp: Timestamp) -> Result<()> {
        match self {
            Self::Active {
                timebase,
                first_timestamp,
                ..
            } => {
                if timestamp.first_timestamp != *first_timestamp
                    || timestamp.timebase_numerator != timebase.numerator()
                    || timestamp.timebase_denominator != timebase.denominator()
                {
                    return Err(FrameExtractorError::TimestampMismatch);
                }
                self.seek_internal(timestamp.timestamp)
            }
            Self::Done => panic!("can't seek when done"),
        }
    }

    fn seek_internal(&mut self, target: i64) -> Result<()> {
        match self {
            Self::Active {
                ictx,
                video_stream_index,
                decoder,
                seek_target_timestamp,
                ..
            } => {
                // TODO: don't seek or undo it if this jumps back, it will just decode the
                // same frames again. Seek to the old frame with AVSEEK_FLAG_ANY?
                seek(ictx, *video_stream_index, target, ..)?;
                decoder.flush();
                *seek_target_timestamp = target;
                Ok(())
            }
            Self::Done => panic!("can't seek when done"),
        }
    }

    pub fn length(&self) -> Duration {
        match self {
            Self::Active {
                end_timestamp,
                first_timestamp,
                timebase,
                ..
            } => timestamp2duration(end_timestamp - first_timestamp, *timebase),
            Self::Done => panic!("can't get length on done"),
        }
    }
}

fn duration2timestamp(dur: Duration, timebase: Rational) -> i64 {
    let step: i64 = dur
        .as_millis()
        .try_into()
        .expect("will probably not be that big");
    let to_seconds = Rational::new(1, 1000);
    // NOTE: This is: step * to_seconds / timebase
    step.rescale(to_seconds, timebase)
}

fn timestamp2duration(timestamp: i64, timebase: Rational) -> Duration {
    let to_seconds = Rational::new(1, 1000);
    // NOTE: timestamp * timebase / to_seconds
    let millis = std::cmp::max(0, timestamp.rescale(timebase, to_seconds));
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
