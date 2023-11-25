use clap::Args;
use image::imageops::{self, crop_imm, flip_horizontal_in_place, FilterType};
use image::math::Rect;
use image::{GenericImageView, GrayImage, ImageBuffer, Pixel, RgbImage, SubImage};

pub use image::imageops::colorops::grayscale;

pub const WHITE: u8 = u8::MAX;
pub const BLACK: u8 = u8::MIN;

pub const DEFAULT_MASKIFY_THRESHOLD: u8 = 40;
pub const DEFAULT_BORDER_MAX_WHITES: f64 = 0.1;

#[derive(Args, Debug)]
pub struct RemoveBordersCli {
    /// All gray values below this becomes black
    #[arg(long, default_value_t = DEFAULT_MASKIFY_THRESHOLD)]
    maskify_threshold: u8,

    /// A mask line can contain this many percent of white and still be considered black
    #[arg(long, default_value_t = DEFAULT_BORDER_MAX_WHITES)]
    maximum_whites: f64,
}

#[derive(Copy, Clone)]
pub struct RemoveBordersArgs {
    maskify_threshold: u8,
    maximum_whites: f64,
}

impl Default for RemoveBordersArgs {
    fn default() -> Self {
        Self {
            maskify_threshold: DEFAULT_MASKIFY_THRESHOLD,
            maximum_whites: DEFAULT_BORDER_MAX_WHITES,
        }
    }
}

impl RemoveBordersArgs {
    pub fn maskify_threshold(mut self, threshold: u8) -> Self {
        self.maskify_threshold = threshold;
        self
    }

    pub fn maximum_whites(mut self, max: f64) -> Self {
        self.maximum_whites = max;
        self
    }

    pub fn remove_borders<'a>(&self, img: &'a RgbImage) -> SubImage<&'a RgbImage> {
        let mask = self.maskify(img);
        let bbox = watermark_getbbox(&mask, self.maximum_whites);
        crop_imm(img, bbox.x, bbox.y, bbox.width, bbox.height)
    }

    pub fn maskify(&self, img: &RgbImage) -> GrayImage {
        maskify(img, self.maskify_threshold)
    }
}

impl RemoveBordersCli {
    pub fn to_args(&self) -> RemoveBordersArgs {
        RemoveBordersArgs::default()
            .maskify_threshold(self.maskify_threshold)
            .maximum_whites(self.maximum_whites)
    }
}

pub fn resize_keep_aspect_ratio<I: GenericImageView>(
    image: &I,
    new_height: u32,
) -> ImageBuffer<I::Pixel, Vec<<I::Pixel as Pixel>::Subpixel>>
where
    I::Pixel: 'static,
    <I::Pixel as Pixel>::Subpixel: 'static,
{
    let new_width = new_width_same_ratio(image.width(), image.height(), new_height);
    imageops::resize(image, new_width, new_height, FilterType::Lanczos3)
}

pub fn worsen_quality<I: GenericImageView>(
    image: &I,
    intermediate_height: u32,
) -> ImageBuffer<I::Pixel, Vec<<I::Pixel as Pixel>::Subpixel>>
where
    I::Pixel: 'static,
    <I::Pixel as Pixel>::Subpixel: 'static,
{
    let intermediate = resize_keep_aspect_ratio(image, intermediate_height);
    imageops::resize(
        &intermediate,
        image.width(),
        image.height(),
        FilterType::Lanczos3,
    )
}

fn new_width_same_ratio(oldw: u32, oldh: u32, newh: u32) -> u32 {
    // TODO: use av_rescale?
    assert_ne!(newh, 0);
    ((newh as u64 * oldw as u64) / oldh as u64) as u32
}

pub fn filled(width: u32, height: u32, red: u8, green: u8, blue: u8) -> RgbImage {
    let mut buf = ImageBuffer::new(width, height);
    buf.enumerate_pixels_mut()
        .for_each(|(_, _, pixel)| *pixel = image::Rgb([red, green, blue]));
    buf
}

pub fn construct_gray(raw: &[&[u8]]) -> GrayImage {
    assert!(raw.windows(2).all(|w| w[0].len() == w[1].len()));
    let height = raw.len() as u32;
    let width = raw.iter().next().map(|row| row.len()).unwrap_or(0) as u32;
    GrayImage::from_fn(width, height, |x, y| {
        image::Luma([raw[y as usize][x as usize]])
    })
}

fn maskify(img: &RgbImage, threshold: u8) -> GrayImage {
    let mut mask = grayscale(img);
    mask.pixels_mut().for_each(|p| {
        p.apply(|bright| (bright <= threshold).then_some(BLACK).unwrap_or(WHITE))
    });
    mask
}

// TODO: use https://crates.io/crates/nalgebra or https://crates.io/crates/ndarray instead
// of manually looping to speed things up?
fn watermark_getbbox(mask: &GrayImage, maximum_whites: f64) -> Rect {
    let maximum_whites = maximum_whites.max(0.0);

    let mut columns = vec![0; mask.width() as usize];
    let mut rows = vec![0; mask.height() as usize];
    mask.enumerate_pixels().for_each(|(x, y, p)| {
        if p[0] == WHITE {
            columns[x as usize] += 1;
            rows[y as usize] += 1;
        }
    });

    let max_col = columns.iter().max().copied().unwrap_or(0);
    let max_row = rows.iter().max().copied().unwrap_or(0);

    let find_border = |axle: &[u64], axle_max: u64| -> Option<u32> {
        if axle.is_empty() || axle_max == 0 {
            return None;
        }

        let axle_max = axle_max as f64;
        axle.iter()
            .enumerate()
            .skip_while(|(_, &w)| ((w as f64) / axle_max) <= maximum_whites)
            .map(|(i, _)| i as u32)
            .next()
    };

    let left = find_border(&columns, max_col).unwrap_or(0);
    columns.reverse();
    let width = find_border(&columns, max_col)
        .map(|right| columns.len() as u32 - right - left)
        .unwrap_or(0);

    let top = find_border(&rows, max_row).unwrap_or(0);
    rows.reverse();
    let height = find_border(&rows, max_row)
        .map(|bottom| rows.len() as u32 - bottom - top)
        .unwrap_or(0);

    Rect {
        x: left,
        y: top,
        width,
        height,
    }
}

pub fn is_subimg_empty<T: GenericImageView>(img: &SubImage<&T>) -> bool {
    is_img_empty(&**img)
}

pub fn is_img_empty<T>(img: &T) -> bool
where
    T: GenericImageView,
{
    img.width() == 0 || img.height() == 0
}

pub fn mirror(mut img: RgbImage) -> RgbImage {
    flip_horizontal_in_place(&mut img);
    img
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn bbox_completely_black() {
        let black = filled(100, 100, 0, 0, 0);
        assert!(black.pixels().all(|p| p[0] == BLACK));

        let mask = maskify(&black, 0);
        assert!(mask.pixels().all(|p| p[0] == BLACK));

        let bbox = watermark_getbbox(&mask, 0.0);
        assert_eq!(
            Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0
            },
            bbox
        );

        let cropped = RemoveBordersArgs::default().remove_borders(&black);
        assert!(is_img_empty(&*cropped));
        assert!(is_subimg_empty(&cropped));
    }

    #[test]
    fn bbox_white() {
        let img = construct_gray(&[&[WHITE, WHITE, WHITE]]);
        let bbox = watermark_getbbox(&img, 0.0);
        assert_eq!(
            Rect {
                x: 0,
                y: 0,
                width: 3,
                height: 1
            },
            bbox
        );
    }

    #[test]
    fn bbox_empty() {
        let img = construct_gray(&[]);
        assert!(is_img_empty(&img));
        let bbox = watermark_getbbox(&img, 0.0);
        assert_eq!(
            Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0
            },
            bbox
        );
    }

    #[test]
    fn bbox_left_edge() {
        let img = construct_gray(&[
            &[BLACK, WHITE, WHITE, WHITE],
            &[BLACK, WHITE, WHITE, WHITE],
            &[BLACK, WHITE, WHITE, WHITE],
            &[BLACK, WHITE, WHITE, WHITE],
        ]);
        let bbox = watermark_getbbox(&img, 0.0);
        assert_eq!(
            Rect {
                x: 1,
                y: 0,
                width: 3,
                height: 4
            },
            bbox
        );
    }

    #[test]
    fn bbox_right_edge() {
        let img = construct_gray(&[
            &[WHITE, WHITE, WHITE, BLACK],
            &[WHITE, WHITE, WHITE, BLACK],
            &[WHITE, WHITE, WHITE, BLACK],
            &[WHITE, WHITE, WHITE, BLACK],
        ]);
        let bbox = watermark_getbbox(&img, 0.0);
        assert_eq!(
            Rect {
                x: 0,
                y: 0,
                width: 3,
                height: 4
            },
            bbox
        );
    }

    #[test]
    fn bbox_top_right_corner() {
        let img = construct_gray(&[
            &[BLACK, BLACK, BLACK, BLACK],
            &[WHITE, WHITE, WHITE, BLACK],
            &[WHITE, WHITE, WHITE, BLACK],
            &[WHITE, WHITE, WHITE, BLACK],
        ]);
        let bbox = watermark_getbbox(&img, 0.0);
        assert_eq!(
            Rect {
                x: 0,
                y: 1,
                width: 3,
                height: 3
            },
            bbox
        );
    }

    #[test]
    fn bbox_surrounded() {
        let img = construct_gray(&[
            &[BLACK, BLACK, BLACK, BLACK],
            &[BLACK, WHITE, WHITE, BLACK],
            &[BLACK, WHITE, WHITE, BLACK],
            &[BLACK, BLACK, BLACK, BLACK],
        ]);
        let bbox = watermark_getbbox(&img, 0.0);
        assert_eq!(
            Rect {
                x: 1,
                y: 1,
                width: 2,
                height: 2
            },
            bbox
        );
    }
}
