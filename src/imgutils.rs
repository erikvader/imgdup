use std::ops::Deref;
use std::path::Path;

use image::imageops::{self, FilterType};
use image::{
    DynamicImage, EncodableLayout, GenericImageView, ImageBuffer, Pixel,
    PixelWithColorType, RgbImage,
};

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
