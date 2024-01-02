extern crate ffmpeg_next as ffmpeg;

use std::cell::RefCell;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use color_eyre::eyre::{self, Context};
use ffmpeg::codec::Context as CodecContext;
use ffmpeg::decoder::Video as DecoderVideo;
use ffmpeg::format::context::Input as FormatContext;
use ffmpeg::format::{input_with_dictionary, Pixel};
use ffmpeg::frame::Video as FrameVideo;
use ffmpeg::media::Type;
use ffmpeg::software::scaling::context::Context as ScalingContext;
use ffmpeg::util::log as ffmpeglog;
use ffmpeg::{Dictionary, Packet as CodecPacket, Rational, Rescale};
use ffmpeg_sys_next::{AV_NOPTS_VALUE, AV_TIME_BASE_Q};
use image::RgbImage;

use super::timestamp::Timestamp;

// TODO: a dedicated error type should probably be preferred here?
pub type Result<T> = eyre::Result<T>;

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
    orientation: Orientation,
}

thread_local! {
    static PATH: RefCell<PathBuf> = RefCell::new(PathBuf::new());
}

macro_rules! path_log {
    ($level:ident, $fmt:literal) => {
        PATH.with_borrow(|path| {
            log::$level!(concat!($fmt, " ({})"), path.display())
        })
    };
    ($level:ident, $fmt:literal, $($arg:tt)+) => {
        PATH.with_borrow(|path| {
            log::$level!(concat!($fmt, " ({})"), $($arg)+, path.display())
        })
    };
    (target: $target:expr, $level:expr, $fmt:literal) => {
        PATH.with_borrow(|path| {
            log::log!(target: $target, $level, concat!($fmt, " ({})"), path.display())
        })
    };
    (target: $target:expr, $level:expr, $fmt:literal, $($arg:tt)+) => {
        PATH.with_borrow(|path| {
            log::log!(target: $target, $level, concat!($fmt, " ({})"), $($arg)+, path.display())
        })
    };
}

impl FrameExtractor {
    // NOTE: Its a little bit ugly that this takes ownership of the path, it should take a
    // reference, but its used in a thread_local that a ffmpeg callback function is using
    // to add what file produced the log message. And also to `Result` messages as
    // context, but the upstream function could do that instead, which is better because
    // it could add them in one spot. Maybe some kind of structured logging is the
    // solution here?

    // TODO: I don't like that this whole module logs. The `next` method could maybe
    // return either a frame or a log message that was produced when retrieving the next
    // frame? It would make things more flexible, but also a little bit messier to
    // implement, which is why it hasn't been done that way currently. Another maybe
    // better solution is to provide a callback closure that gets called everytime there
    // is a log message?
    pub fn new<P: Into<PathBuf>>(path: P) -> Result<Self> {
        if let Err(e) = FFMPEG_INITIALIZED.get_or_init(|| {
            ffmpeg::init()?;
            ffmpeglog::set_level(ffmpeglog::Level::Warning);
            unsafe {
                ffmpeg_sys_next::av_log_set_callback(Some(ffmpeg_log_adaptor));
            }
            Ok(())
        }) {
            return Err(e).wrap_err("Failed to initialize ffmpeg");
        }

        // NOTE: This is the only place where this is set, it is only read everywhere
        // else. This assumes that there is only one frame extractor running per thread,
        // else one of them would use the wrong path. They shouldn't be able to clash with
        // each other and cause panics.
        PATH.set(path.into());
        let s = PATH.with_borrow(|path| {
            Self::new_inner(&path).wrap_err_with(|| format!("on file {:?}", path))
        })?;
        Ok(s)
    }

    fn new_inner(path: &Path) -> Result<Self> {
        let options = {
            let mut options = Dictionary::new();
            options.set("analyzeduration", "10M");
            options.set("probesize", "5M"); // this is the default
            options
        };
        let mut ictx =
            input_with_dictionary(&path, options).wrap_err("Failed to open the file")?;

        let video = ictx
            .streams()
            .best(Type::Video)
            .ok_or(eyre::eyre!("No video stream"))?;

        let video_stream_index = video.index();
        eyre::ensure!(
            video.start_time() != AV_NOPTS_VALUE,
            "Does not have a start time"
        );
        let cur_timestamp = video.start_time();
        let seek_target_timestamp = video.start_time();
        let first_timestamp = video.start_time();
        let timebase = video.time_base();
        let end_timestamp = if video.duration() == AV_NOPTS_VALUE {
            eyre::ensure!(
                ictx.duration() != AV_NOPTS_VALUE,
                "Does not have a duration"
            );
            ictx.duration().rescale(AV_TIME_BASE_Q, timebase)
        } else {
            video.duration()
        };
        eyre::ensure!(
            end_timestamp >= cur_timestamp,
            "The end timestamp is less than the start"
        );

        let orientation = get_orientation(&video);

        let decoder = CodecContext::from_parameters(video.parameters())
            .wrap_err("No codec found")?
            .decoder()
            .video()
            .wrap_err("No codec found, of type video (?)")?;

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
            orientation,
        })
    }

    fn pixel_converter(decoder: &DecoderVideo) -> Result<ScalingContext> {
        eyre::ensure!(decoder.format() != Pixel::None, "No pixel format");
        Ok(ScalingContext::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            // http://git.videolan.org/?p=ffmpeg.git;a=blob;f=libavutil/pixfmt.h;hb=HEAD
            Pixel::RGB24,
            decoder.width(),
            decoder.height(),
            ffmpeg::software::scaling::Flags::FAST_BILINEAR,
        )?)
    }

    pub fn next(&mut self) -> Result<Option<(Timestamp, RgbImage)>> {
        self.next_inner()
            .wrap_err_with(|| PATH.with_borrow(|path| format!("on file {:?}", path)))
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
            orientation,
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
                        return Err(e)
                            .wrap_err("Decoder error when receiving a frame from it");
                    }
                }

                if let Some(ts) = frame.timestamp() {
                    *cur_timestamp = ts;
                } else {
                    let dur = Timestamp::new(*cur_timestamp, *timebase, *first_timestamp);
                    path_log!(
                        warn,
                        "Frame doesn't have a timestamp somewhere after: {}",
                        dur
                    );
                    continue;
                }

                if *cur_timestamp < *seek_target_timestamp {
                    continue;
                }

                let mut converted = FrameVideo::empty();
                converter
                    .run(&frame, &mut converted)
                    .wrap_err("Failed to convert the decoded frame")?;
                let img = create_rust_image(converted);
                let img = undo_rotation(img, *orientation);

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
                                path_log!(error, "Failed to decode frame: {}", e);
                                continue;
                            }
                        }
                    }
                    Ok(()) => continue,
                    Err(ffmpeg::Error::Eof) => {
                        decoder
                            .send_eof()
                            .wrap_err("Failed to send EOF to the decoder")?;
                        break;
                    }
                    Err(e) => {
                        eyre::bail!("Failed to read a packet from the stream: {e}");
                    }
                }
            }
        }
    }

    pub fn seek_forward(&mut self, dur: Duration) -> Result<()> {
        if dur.is_zero() {
            return Ok(());
        }

        let target =
            self.cur_timestamp + Timestamp::from_duration(dur).timestamp(self.timebase);
        // TODO: don't seek, or undo it, if this jumps back, it will just decode
        // the same frames again. Seek to the old frame with AVSEEK_FLAG_ANY?
        self.seek_internal(target).wrap_err_with(|| {
            format!(
                "Failed when trying to seek forward {} from {}",
                humantime::Duration::from(dur),
                humantime::Duration::from(
                    Timestamp::new_abs(self.cur_timestamp, self.timebase).to_duration()
                )
            )
        })
    }

    pub fn seek_to(&mut self, timestamp: &Timestamp) -> Result<()> {
        if timestamp.first_timestamp != self.first_timestamp
            || timestamp.timebase_numerator != self.timebase.numerator()
            || timestamp.timebase_denominator != self.timebase.denominator()
        {
            eyre::bail!("The given timestamp does not seem to match with the file");
        }

        self.seek_internal(timestamp.timestamp)
            .wrap_err_with(|| format!("Failed when seeking to {:?}", timestamp))
    }

    pub fn seek_to_beginning(&mut self) -> Result<()> {
        self.seek_internal(self.first_timestamp).wrap_err_with(|| {
            format!(
                "Failed to seek to the beginning at {}",
                self.first_timestamp
            )
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

        seek(ictx, *video_stream_index, target, ..).wrap_err_with(|| {
            PATH.with_borrow(|path| format!("Failed to seek on file {:?}", path))
        })?;
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

        Timestamp::new(*end_timestamp, *timebase, *first_timestamp).to_duration()
    }
}

#[derive(Clone, Copy)]
enum Orientation {
    Normal,
    Left,
    Right,
    Upside,
}

fn get_orientation(video: &ffmpeg::Stream) -> Orientation {
    // TODO: Rotation can also be set in the metadata dict, find an example video and fix!
    for data in video.side_data() {
        if data.kind() != ffmpeg::packet::side_data::Type::DisplayMatrix {
            continue;
        }
        let rot = unsafe {
            ffmpeg_sys_next::av_display_rotation_get(data.data().as_ptr() as *const i32)
        };

        if rot.is_finite() {
            return match rot.round() as i32 {
                -90 => Orientation::Right,
                90 => Orientation::Left,
                0 => Orientation::Normal,
                180 | -180 => Orientation::Upside,
                x => {
                    // TODO: should this even be logged?
                    path_log!(warn, "Weird orientation angle: {}", x);
                    Orientation::Normal
                }
            };
        }
    }

    Orientation::Normal
}

fn undo_rotation(img: RgbImage, ori: Orientation) -> RgbImage {
    match ori {
        Orientation::Normal => img,
        Orientation::Right => image::imageops::rotate90(&img),
        Orientation::Left => image::imageops::rotate270(&img),
        Orientation::Upside => image::imageops::rotate180(&img),
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

impl<'a> FrameExtractor {
    pub fn iter(&'a mut self) -> FrameExtractorIter<'a> {
        FrameExtractorIter { extractor: self }
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
            ..
        } = self;

        let mut debug_struct = f.debug_struct("FrameExtractor::Active");
        debug_struct
            .field("first_ts", first_timestamp)
            .field("last_ts", end_timestamp)
            .field("cur_ts", cur_timestamp)
            .field(
                "tb",
                &format_args!("{}/{}", timebase.numerator(), timebase.denominator()),
            )
            .field("seek_ts", seek_target_timestamp);

        PATH.with_borrow(|path| debug_struct.field("path", path));

        debug_struct.finish()
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
        if avcl.is_null() {
            "NULL_avcl".into()
        } else {
            let avc = *(avcl as *const *const ffmpeg_sys_next::AVClass);
            if avc.is_null() {
                "NULL_avc".into()
            } else {
                if let Some(fun) = (*avc).item_name {
                    let item = std::ffi::CStr::from_ptr(fun(avcl)).to_string_lossy();
                    if item == "NULL" {
                        std::ffi::CStr::from_ptr((*avc).class_name).to_string_lossy()
                    } else {
                        item
                    }
                } else {
                    "NULL_item".into()
                }
            }
        }
    };

    let target = format!("ffmpeg::{class_name}");
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

    path_log!(target: &target, level, "{}", rust_str);
}