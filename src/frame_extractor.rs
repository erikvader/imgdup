extern crate ffmpeg_next as ffmpeg;

pub mod timestamp;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use std::{fmt, ptr};

use error_stack::{bail, report, IntoReport, Report, ResultExt};
use ffmpeg::codec::Context as CodecContext;
use ffmpeg::decoder::Video as DecoderVideo;
use ffmpeg::format::context::Input as FormatContext;
use ffmpeg::format::{input_with_dictionary, Pixel};
use ffmpeg::frame::Video as FrameVideo;
use ffmpeg::software::scaling::context::Context as ScalingContext;
use ffmpeg::util::log as ffmpeglog;
use ffmpeg::{format::input, media::Type};
use ffmpeg::{Dictionary, Packet as CodecPacket, Rational, Rescale};
use ffmpeg_sys_next::{AV_NOPTS_VALUE, AV_TIME_BASE_Q};
use image::RgbImage;

use self::timestamp::Timestamp;

#[derive(thiserror::Error, Debug)]
pub enum FrameExtractorError {
    #[error("Timestamp does not seem to originate from the same file")]
    TimestampMismatch,
    #[error("Cannot initialize ffmpeg")]
    FfmpegInitError,
    #[error("Failed to open")]
    FailedToOpen,
    #[error("No video stream")]
    NoVideoStream,
    #[error("Failed to open codec")]
    NoCodec,
    #[error("Failed to seek")]
    Seek,
    #[error("Failed to decode frame")]
    Decode,
}

pub type Result<T> = error_stack::Result<T, FrameExtractorError>;

static FFMPEG_INITIALIZED: OnceLock<std::result::Result<(), ffmpeg::Error>> =
    OnceLock::new();

pub struct FrameExtractor {
    // TODO: probably split into several structs
    // ffmpeg contexts
    ictx: FormatContext,
    decoder: DecoderVideo,
    converter: ScalingContext,

    // internal timestamp bookkeeping
    seek_target_timestamp: i64,
    cur_timestamp: i64,

    // constants/metadata
    end_timestamp: i64,
    first_timestamp: i64,
    timebase: Rational,
    video_stream_index: usize,

    // the file name
    path: PathBuf,
}

impl FrameExtractor {
    pub fn new(path: PathBuf) -> Result<Self> {
        if let Err(e) = FFMPEG_INITIALIZED.get_or_init(|| {
            ffmpeg::init()?;
            ffmpeglog::set_level(ffmpeglog::Level::Warning);
            unsafe {
                ffmpeg_sys_next::av_log_set_callback(Some(ffmpeg_log_adaptor));
            }
            Ok(())
        }) {
            return Err(report!(e).change_context(FrameExtractorError::FfmpegInitError));
        }

        let mut s = Self::new_inner(&path)
            .attach_printable_lazy(|| format!("on file {:?}", path))?;
        s.path = path; // NOTE: ugly workaround to avoid copying the path
        Ok(s)
    }

    fn new_inner(path: &Path) -> Result<Self> {
        let options = {
            let mut options = Dictionary::new();
            options.set("analyzeduration", "10M");
            options.set("probesize", "5M"); // this is the default
            options
        };
        let mut ictx = input_with_dictionary(&path, options)
            .into_report()
            .change_context(FrameExtractorError::FailedToOpen)?;

        // TODO: somehow set the discard property on everything, except the video, to
        // improve seeking. There doesn't seem to be a way to do this in ffmpeg-next 6.0.0
        let video = ictx
            .streams()
            .best(Type::Video)
            .ok_or(FrameExtractorError::NoVideoStream)
            .into_report()?;

        let video_stream_index = video.index();
        assert_ne!(AV_NOPTS_VALUE, video.start_time());
        let cur_timestamp = video.start_time();
        let seek_target_timestamp = video.start_time();
        let first_timestamp = video.start_time();
        let timebase = video.time_base();
        let end_timestamp = if video.duration() == AV_NOPTS_VALUE {
            assert_ne!(AV_NOPTS_VALUE, ictx.duration());
            ictx.duration().rescale(AV_TIME_BASE_Q, timebase)
        } else {
            video.duration()
        };

        let decoder = CodecContext::from_parameters(video.parameters())
            .into_report()
            .change_context(FrameExtractorError::NoCodec)?
            .decoder()
            .video()
            .into_report()
            .change_context(FrameExtractorError::NoCodec)?;

        let converter = Self::pixel_converter(&decoder)?;

        ictx.streams_mut()
            .filter(|stream| stream.index() != video_stream_index)
            .for_each(|mut stream| stream_set_discard_all(&mut stream));

        Ok(Self {
            ictx,
            decoder,
            video_stream_index,
            converter,
            cur_timestamp,
            end_timestamp,
            seek_target_timestamp,
            first_timestamp,
            timebase,
            path: PathBuf::new(),
        })
    }

    fn pixel_converter(decoder: &DecoderVideo) -> Result<ScalingContext> {
        assert_ne!(Pixel::None, decoder.format());
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
        .into_report()
        .change_context(FrameExtractorError::NoCodec)
    }

    pub fn iter(&mut self) -> FrameExtractorIter {
        FrameExtractorIter { extractor: self }
    }

    pub fn next(&mut self) -> Result<Option<(Timestamp, RgbImage)>> {
        self.next_inner()
            .attach_printable_lazy(|| format!("on file {:?}", self.path))
    }

    fn next_inner(&mut self) -> Result<Option<(Timestamp, RgbImage)>> {
        let Self {
            ictx,
            decoder,
            video_stream_index,
            converter,
            cur_timestamp,
            seek_target_timestamp,
            timebase,
            first_timestamp,
            ..
        } = self;

        loop {
            loop {
                let mut frame = FrameVideo::empty();
                // avcodec_receive_frame
                // https://ffmpeg.org/doxygen/trunk/group__lavc__decoding.html#ga11e6542c4e66d3028668788a1a74217c
                match decoder.receive_frame(&mut frame) {
                    Ok(()) => (),
                    Err(ffmpeg::Error::Other {
                        errno: libc::EAGAIN,
                    }) => break,
                    // End of stream situations.
                    // https://ffmpeg.org/doxygen/trunk/avcodec_8h_source.html
                    Err(ffmpeg::Error::Eof) => return Ok(None),
                    Err(e) => {
                        return Err(report!(e).change_context(FrameExtractorError::Decode))
                    }
                }

                *cur_timestamp = frame
                    .timestamp()
                    .expect("this is always set by the decoder?");

                if *cur_timestamp < *seek_target_timestamp {
                    continue;
                }

                let mut converted = FrameVideo::empty();
                converter
                    .run(&frame, &mut converted)
                    .into_report()
                    .change_context(FrameExtractorError::Decode)?;
                let img = create_rust_image(converted);

                let dur = Timestamp::new(*cur_timestamp, *timebase, *first_timestamp);
                return Ok(Some((dur, img)));
            }

            loop {
                // http://ffmpeg.org/doxygen/trunk/group__lavf__decoding.html#ga4fdb3084415a82e3810de6ee60e46a61
                let mut packet = CodecPacket::empty();
                match packet.read(ictx) {
                    Ok(()) if packet.stream() == *video_stream_index => {
                        match decoder.send_packet(&packet) {
                            Ok(()) => break,
                            Err(e) => {
                                log::error!("Failed to decode frame: {e}");
                                continue;
                            }
                        }
                    }
                    Ok(()) => continue,
                    Err(ffmpeg::Error::Eof) => {
                        decoder
                            .send_eof()
                            .into_report()
                            .change_context(FrameExtractorError::Decode)?;
                        break;
                    }
                    Err(e) => {
                        return Err(report!(e).change_context(FrameExtractorError::Decode))
                    }
                }
            }
        }
    }

    pub fn seek_forward(&mut self, dur: Duration) -> Result<()> {
        if dur.is_zero() {
            return Ok(());
        }

        let target = self.cur_timestamp + duration2timestamp(dur, self.timebase);
        // TODO: don't seek, or undo it, if this jumps back, it will just decode
        // the same frames again. Seek to the old frame with AVSEEK_FLAG_ANY?
        self.seek_internal(target).attach_printable_lazy(|| {
            format!(
                "tried to seek forward {} from {}",
                humantime::Duration::from(dur),
                humantime::Duration::from(timestamp2duration(
                    self.cur_timestamp,
                    self.timebase
                ))
            )
        })
    }

    pub fn seek_to(&mut self, timestamp: Timestamp) -> Result<()> {
        if timestamp.first_timestamp != self.first_timestamp
            || timestamp.timebase_numerator != self.timebase.numerator()
            || timestamp.timebase_denominator != self.timebase.denominator()
        {
            bail!(FrameExtractorError::TimestampMismatch)
        }

        self.seek_internal(timestamp.timestamp)
            .attach_printable_lazy(|| format!("seeking to {:?}", timestamp))
    }

    pub fn seek_to_beginning(&mut self) -> Result<()> {
        self.seek_internal(self.first_timestamp)
            .attach_printable_lazy(|| {
                format!("seeking to the beginning at {}", self.first_timestamp)
            })
    }

    fn seek_internal(&mut self, target: i64) -> Result<()> {
        let Self {
            ictx,
            video_stream_index,
            decoder,
            seek_target_timestamp,
            ..
        } = self;

        seek(ictx, *video_stream_index, target, ..)
            .into_report()
            .change_context(FrameExtractorError::Seek)
            .attach_printable_lazy(|| format!("on file {:?}", self.path))?;
        decoder.flush();
        *seek_target_timestamp = target;
        Ok(())
    }

    pub fn approx_length(&self) -> Duration {
        let Self {
            end_timestamp,
            first_timestamp,
            timebase,
            ..
        } = self;

        timestamp2duration(end_timestamp - first_timestamp, *timebase)
    }
}

pub struct FrameExtractorIter<'a> {
    extractor: &'a mut FrameExtractor,
}

impl Iterator for FrameExtractorIter<'_> {
    type Item = Result<(Timestamp, RgbImage)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.extractor.next().transpose()
    }
}

fn create_rust_image(converted: FrameVideo) -> RgbImage {
    assert_eq!(Pixel::RGB24, converted.format());
    assert_eq!(1, converted.planes());

    let src_linesize = converted.stride(0);
    let width: usize = converted.width().try_into().expect("will always fit");
    let height: usize = converted.height().try_into().expect("will always fit");
    let data = converted.data(0);
    let trg_linesize = 3 * width;

    // https://stackoverflow.com/a/57666844
    let data = if src_linesize == trg_linesize {
        data.to_vec()
    } else {
        assert!(src_linesize >= trg_linesize);
        let mut nopadding = vec![0; trg_linesize * height];
        for i in 0..height {
            nopadding[(i * trg_linesize)..((i + 1) * trg_linesize)].copy_from_slice(
                &data[(i * src_linesize)..(i * src_linesize + trg_linesize)],
            );
        }
        nopadding
    };

    RgbImage::from_vec(
        width.try_into().expect("was an u32 before"),
        height.try_into().expect("was an u32 before"),
        data,
    )
    .expect("the buffer is big enough!")
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

fn stream_set_discard_all(stream: &mut ffmpeg::StreamMut<'_>) {
    unsafe {
        let ptr = stream.as_mut_ptr();
        if !ptr.is_null() {
            (*ptr).discard = ffmpeg_sys_next::AVDiscard::AVDISCARD_ALL;
        }
    }
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

impl fmt::Debug for FrameExtractor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            first_timestamp,
            end_timestamp,
            timebase,
            cur_timestamp,
            seek_target_timestamp,
            path,
            ..
        } = self;

        f.debug_struct("FrameExtractor::Active")
            .field("first_ts", first_timestamp)
            .field("last_ts", end_timestamp)
            .field("cur_ts", cur_timestamp)
            .field(
                "tb",
                &format_args!("{}/{}", timebase.numerator(), timebase.denominator()),
            )
            .field("seek_ts", seek_target_timestamp)
            .field("path", path)
            .finish()
    }
}

extern "C" {
    pub fn vsnprintf(
        strbuf: *mut libc::c_char,
        size: libc::size_t,
        format: *const libc::c_char,
        va_list: *mut libc::c_void,
    ) -> libc::c_int;
}

unsafe extern "C" fn ffmpeg_log_adaptor(
    avcl: *mut libc::c_void,
    level: libc::c_int,
    fmt: *const libc::c_char,
    va_list: *mut ffmpeg_sys_next::__va_list_tag,
) {
    if level > ffmpeg_sys_next::av_log_get_level() {
        return;
    }

    const BUF_SIZE: usize = 2048;
    let mut buffer: Vec<u8> = vec![1; BUF_SIZE];
    let retval = vsnprintf(
        buffer.as_mut_ptr() as *mut libc::c_char,
        BUF_SIZE,
        fmt,
        va_list as *mut libc::c_void,
    );

    match retval {
        ..=-1 => {
            let errno = std::io::Error::last_os_error();
            eprintln!(
                "failed to create log message from ffmpeg, vsnprintf returned: {errno}"
            );
            return;
        }
        written => {
            buffer.pop();
            buffer.truncate(written.try_into().expect("is not negative"))
        }
    }

    let mut rust_str = match String::from_utf8_lossy(&buffer) {
        std::borrow::Cow::Borrowed(_) => String::from_utf8_unchecked(buffer),
        std::borrow::Cow::Owned(s) => s,
    };
    rust_str.truncate(rust_str.trim_end().len());

    let class_name = {
        if !avcl.is_null() {
            let avcl = *(avcl as *const *const ffmpeg_sys_next::AVClass);
            std::ffi::CStr::from_ptr((*avcl).class_name).to_string_lossy()
        } else {
            "NULL".into()
        }
    };

    let target = format!("{}::ffmpeg::'{class_name}'", module_path!());
    let level = match ffmpeglog::Level::try_from(level) {
        Ok(ffmpeglog::Level::Verbose) => log::Level::Trace,
        Ok(ffmpeglog::Level::Trace) => log::Level::Trace,
        Ok(ffmpeglog::Level::Debug) => log::Level::Debug,
        Ok(ffmpeglog::Level::Error) => log::Level::Error,
        Ok(ffmpeglog::Level::Fatal) => log::Level::Error,
        Ok(ffmpeglog::Level::Panic) => log::Level::Error,
        Ok(ffmpeglog::Level::Warning) => log::Level::Warn,
        Ok(ffmpeglog::Level::Quiet) => log::Level::Info,
        Ok(ffmpeglog::Level::Info) => log::Level::Info,
        Err(_) => log::Level::Info,
    };

    log::log!(target: &target, level, "{rust_str}");
}
