use image::imageops::{self, crop_imm, flip_horizontal_in_place, FilterType};
use image::math::Rect;
use image::{GenericImageView, GrayImage, ImageBuffer, Pixel, Rgb, RgbImage, SubImage};

pub use image::imageops::colorops::grayscale;

use super::args_helper::args;
use super::math::{Average, Variance};

pub const WHITE: u8 = u8::MAX;
pub const BLACK: u8 = u8::MIN;

args! {
    #[derive(Copy, Clone)]
    RemoveBorders {
        "All gray values below this becomes black"
        maskify_threshold: u8 = 40;

        "A mask line can contain this many percent of white and still be considered black"
        maximum_whites: f64 = 0.1;
    }
}

impl RemoveBordersArgs {
    pub fn remove_borders<'a>(self, img: &'a RgbImage) -> SubImage<&'a RgbImage> {
        let mask = self.maskify(img);
        self.remove_borders_mask(img, &mask)
    }

    pub fn remove_borders_mask<'a>(
        self,
        img: &'a RgbImage,
        mask: &GrayImage,
    ) -> SubImage<&'a RgbImage> {
        let bbox = watermark_getbbox(&mask, self.maximum_whites);
        crop_imm(img, bbox.x, bbox.y, bbox.width, bbox.height)
    }

    pub fn maskify(self, img: &RgbImage) -> GrayImage {
        maskify(img, self.maskify_threshold)
    }
}

args! {
    #[derive(Copy, Clone)]
    Blandness {
        "Images with blandess less than or equal to this are filetered out (negative to disable)"
        blandness_threshold: f64 = -1.0;
    }
}

impl BlandnessArgs {
    pub fn blandness<I>(self, img: &I) -> f64
    where
        I: GenericImageView<Pixel = Rgb<u8>>,
    {
        color_variance(img)
    }

    pub fn is_value_bland(self, blandness: f64) -> bool {
        blandness <= self.blandness_threshold
    }

    pub fn is_bland<I>(self, img: &I) -> bool
    where
        I: GenericImageView<Pixel = Rgb<u8>>,
    {
        self.blandness_threshold >= 0.0 && self.is_value_bland(self.blandness(img))
    }
}

args! {
    #[derive(Copy, Clone)]
    BlackMask {
        "Masks that are at least this many percent black are filtered out (negative to disable)"
        black_mask_threshold: f64 = 90.0;
    }
}

impl BlackMaskArgs {
    pub fn blackness(self, mask: &GrayImage) -> f64 {
        mask_blackness(mask)
    }

    pub fn is_value_too_black(self, blackness: f64) -> bool {
        blackness >= self.black_mask_threshold
    }

    pub fn is_too_black(self, mask: &GrayImage) -> bool {
        self.black_mask_threshold >= 0.0 && self.is_value_too_black(self.blackness(mask))
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

pub fn maskify(img: &RgbImage, threshold: u8) -> GrayImage {
    let mut mask = grayscale(img);
    mask.pixels_mut().for_each(|p| {
        p.apply(|bright| (bright <= threshold).then_some(BLACK).unwrap_or(WHITE))
    });
    mask
}

pub fn mask_blackness(img: &GrayImage) -> f64 {
    let black_count = img.pixels().filter(|p| p[0] == BLACK).count();
    let total = img.width() * img.height();
    100.0 * (black_count as f64) / (total as f64)
}

// TODO: use https://crates.io/crates/nalgebra or https://crates.io/crates/ndarray instead
// of manually looping to speed things up?
pub fn watermark_getbbox(mask: &GrayImage, maximum_whites: f64) -> Rect {
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

// https://sighack.com/post/averaging-rgb-colors-the-right-way
pub fn average_color<I>(img: &I) -> Rgb<u8>
where
    I: GenericImageView<Pixel = Rgb<u8>>,
{
    let mut red = Average::new();
    let mut green = Average::new();
    let mut blue = Average::new();

    img.pixels().for_each(|(_, _, rgb)| {
        red.add(rgb[0] as u16 * rgb[0] as u16);
        green.add(rgb[1] as u16 * rgb[1] as u16);
        blue.add(rgb[2] as u16 * rgb[2] as u16);
    });

    let red = red.average().sqrt() as u8;
    let green = green.average().sqrt() as u8;
    let blue = blue.average().sqrt() as u8;
    Rgb([red, green, blue])
}

pub fn rgb_dist(a: Rgb<u8>, b: Rgb<u8>) -> f64 {
    fn square(a: u8, b: u8) -> f64 {
        let a: f64 = a.into();
        let b: f64 = b.into();
        let x = a - b;
        x * x
    }
    (square(a[0], b[0]) + square(a[1], b[1]) + square(a[2], b[2])).sqrt()
}

pub fn color_variance<I>(img: &I) -> f64
where
    I: GenericImageView<Pixel = Rgb<u8>>,
{
    let avg_col = average_color(img);
    let mut var = Variance::new();

    img.pixels().for_each(|(_, _, rgb)| {
        var.add(rgb_dist(rgb, avg_col));
    });

    var.variance()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn avg_color() {
        let black = filled(100, 100, 0, 0, 0);
        assert_eq!(Rgb([0, 0, 0]), average_color(&black));

        let white = filled(100, 100, 255, 255, 255);
        assert_eq!(Rgb([255, 255, 255]), average_color(&white));
    }

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
