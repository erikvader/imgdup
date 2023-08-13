use std::ops::Deref;
use std::path::Path;

use image::imageops::{self, crop_imm, FilterType};
use image::math::Rect;
use image::{
    DynamicImage, EncodableLayout, GenericImageView, GrayImage, ImageBuffer, Pixel,
    PixelWithColorType, RgbImage, SubImage,
};

pub use image::imageops::colorops::grayscale;

const WHITE: u8 = u8::MAX;
const BLACK: u8 = u8::MIN;

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

pub fn remove_borders(
    img: &RgbImage,
    maskify_threshold: u8,
    maximum_whites: f64,
) -> SubImage<&RgbImage> {
    let mask = maskify(img, maskify_threshold);
    let bbox = watermark_getbbox(&mask, maximum_whites);
    crop_imm(img, bbox.x, bbox.y, bbox.width, bbox.height)
}

pub fn maskify(img: &RgbImage, threshold: u8) -> GrayImage {
    let mut mask = grayscale(img);
    mask.pixels_mut().for_each(|p| {
        p.apply(|bright| (bright <= threshold).then_some(BLACK).unwrap_or(WHITE))
    });
    mask
}

pub fn watermark_getbbox(mask: &GrayImage, maximum_whites: f64) -> Rect {
    let mut columns = vec![0; mask.width() as usize];
    let mut rows = vec![0; mask.height() as usize];
    mask.enumerate_pixels().for_each(|(x, y, p)| {
        if p[0] == WHITE {
            columns[x as usize] += 1;
            rows[y as usize] += 1;
        }
    });

    fn find_border(counts: &[u64], maximum_whites: f64) -> Option<u32> {
        if counts.is_empty() {
            return None;
        }

        let len = counts.len() as f64;
        counts
            .iter()
            .enumerate()
            .skip_while(|(_, &w)| ((w as f64) / len) <= maximum_whites)
            .map(|(i, _)| i as u32)
            .next()
    }

    let left = find_border(&columns, maximum_whites).unwrap_or(0);
    columns.reverse();
    let width = find_border(&columns, maximum_whites)
        .map(|right| columns.len() as u32 - right - left)
        .unwrap_or(0);

    let top = find_border(&rows, maximum_whites).unwrap_or(0);
    rows.reverse();
    let height = find_border(&rows, maximum_whites)
        .map(|bottom| rows.len() as u32 - bottom - top)
        .unwrap_or(0);

    Rect {
        x: left,
        y: top,
        width,
        height,
    }
}

pub fn subimg_empty<T: GenericImageView>(img: &SubImage<&T>) -> bool {
    img_empty(&**img)
}

pub fn img_empty<T>(img: &T) -> bool
where
    T: GenericImageView,
{
    img.width() == 0 || img.height() == 0
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

        let cropped = remove_borders(&black, 0, 0.0);
        assert!(img_empty(&*cropped));
        assert!(subimg_empty(&cropped));
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
        assert!(img_empty(&img));
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
