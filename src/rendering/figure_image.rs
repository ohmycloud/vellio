//! Display-only paper removal for PDF/EPUB diagrams; source assets stay untouched.
use image::RgbaImage;

fn white(pixel: &image::Rgba<u8>) -> bool {
    pixel[3] == 255 && pixel[0] >= 250 && pixel[1] >= 250 && pixel[2] >= 250
}

/// Remove a broad white matte connected to the crop edge. Enclosed white
/// objects, colored panels, and images without a white paper border survive.
pub fn transparent_paper(mut image: RgbaImage) -> RgbaImage {
    let (w, h) = image.dimensions();
    if w < 3 || h < 3 {
        return image;
    }
    let mut edge = Vec::new();
    for x in 0..w {
        edge.push((x, 0));
        edge.push((x, h - 1));
    }
    for y in 1..h - 1 {
        edge.push((0, y));
        edge.push((w - 1, y));
    }
    let mut pending: Vec<_> = edge
        .iter()
        .copied()
        .filter(|&(x, y)| white(image.get_pixel(x, y)))
        .collect();
    if pending.len() * 5 < edge.len() {
        return image;
    }
    let mut paper = vec![false; w as usize * h as usize];
    let mut count = 0;
    while let Some((x, y)) = pending.pop() {
        let ix = y as usize * w as usize + x as usize;
        if paper[ix] || !white(image.get_pixel(x, y)) {
            continue;
        }
        paper[ix] = true;
        count += 1;
        if x > 0 {
            pending.push((x - 1, y));
        }
        if x + 1 < w {
            pending.push((x + 1, y));
        }
        if y > 0 {
            pending.push((x, y - 1));
        }
        if y + 1 < h {
            pending.push((x, y + 1));
        }
    }
    // Avoid changing a photograph just because a few edge pixels are white.
    if count * 10 < paper.len() {
        return image;
    }
    for y in 0..h {
        for x in 0..w {
            let ix = y as usize * w as usize + x as usize;
            let pixel = image.get_pixel_mut(x, y);
            if paper[ix] {
                *pixel = image::Rgba([0, 0, 0, 0]);
            } else if pixel[3] == 255
                && (x > 0 && paper[ix - 1]
                    || x + 1 < w && paper[ix + 1]
                    || y > 0 && paper[ix - w as usize]
                    || y + 1 < h && paper[ix + w as usize])
            {
                // Undo white matting on the first antialiased edge pixel.
                // This reconstructs its color over white, without a pale halo
                // when the reader's warm paper shows through instead.
                let matte = pixel[0].min(pixel[1]).min(pixel[2]);
                let alpha = 255 - matte;
                if alpha > 0 {
                    for c in 0..3 {
                        pixel[c] = ((pixel[c] - matte) as u16 * 255 / alpha as u16) as u8;
                    }
                    pixel[3] = alpha;
                }
            }
        }
    }
    image
}

/// Shared reader/preview preparation. Source files are never rewritten.
/// GIFs keep their original representation rather than flattening animation.
pub fn display_bytes(source_kind: &str, bytes: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    if !matches!(source_kind, "pdf-image" | "pdf-page" | "epub-image")
        || image::guess_format(&bytes)? == image::ImageFormat::Gif
    {
        return Ok(bytes);
    }
    let rgba = image::load_from_memory(&bytes)?.to_rgba8();
    let rgba = transparent_paper(rgba);
    let mut png = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut png, image::ImageFormat::Png)?;
    Ok(png.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn connected_paper_clears_but_enclosed_white_and_ink_survive() {
        let mut input = RgbaImage::from_pixel(20, 20, Rgba([255, 255, 255, 255]));
        for y in 5..15 {
            for x in 5..15 {
                input.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        input.put_pixel(9, 9, Rgba([255, 255, 255, 255]));
        input.put_pixel(4, 8, Rgba([180, 180, 180, 255]));
        let output = transparent_paper(input);
        assert_eq!(output.get_pixel(0, 0)[3], 0);
        assert_eq!(output.get_pixel(9, 9).0, [255, 255, 255, 255]);
        assert_eq!(output.get_pixel(8, 8).0, [0, 0, 0, 255]);
        assert_eq!(output.get_pixel(4, 8).0, [0, 0, 0, 75]);
    }

    #[test]
    fn epub_png_and_jpeg_figures_use_display_only_paper_removal() {
        let mut source = RgbaImage::from_pixel(40, 40, Rgba([255, 255, 255, 255]));
        for y in 10..30 {
            for x in 10..30 {
                source.put_pixel(x, y, Rgba([30, 70, 110, 255]));
            }
        }
        source.put_pixel(20, 20, Rgba([255, 255, 255, 255]));
        for format in [image::ImageFormat::Png, image::ImageFormat::Jpeg] {
            let mut original = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(source.clone())
                .to_rgb8()
                .write_to(&mut original, format)
                .unwrap();
            let bytes = original.into_inner();
            let display = display_bytes("epub-image", bytes.clone()).unwrap();
            let rgba = image::load_from_memory(&display).unwrap().to_rgba8();
            assert_eq!(rgba.get_pixel(0, 0)[3], 0);
            assert_eq!(rgba.get_pixel(20, 20)[3], 255);
            assert_eq!(rgba.get_pixel(15, 15)[3], 255);
            assert_eq!(display_bytes("other-image", bytes.clone()).unwrap(), bytes);
        }
    }

    #[test]
    fn colored_epub_photo_display_preserves_pixels() {
        let original =
            RgbaImage::from_fn(40, 40, |x, y| Rgba([40 + x as u8, 70 + y as u8, 130, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        original
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let display = display_bytes("epub-image", png.into_inner()).unwrap();
        assert_eq!(
            image::load_from_memory(&display).unwrap().to_rgba8(),
            original
        );
    }

    #[test]
    fn transparent_diagrams_and_colored_photos_keep_their_pixels() {
        for color in [[70, 120, 160, 255], [0, 0, 0, 0]] {
            let mut input = RgbaImage::from_pixel(20, 20, Rgba(color));
            input.put_pixel(0, 0, Rgba([255, 255, 255, 255]));
            input.put_pixel(10, 10, Rgba([255, 255, 255, 255]));
            assert_eq!(transparent_paper(input.clone()), input);
        }
    }
}
