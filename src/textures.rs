mod resample;

use std::path::Path;

use anyhow::{Result, ensure};
use image::{DynamicImage, GenericImage, GenericImageView, ImageDecoder, RgbaImage, SubImage};

use crate::config::{Scale, TextureOptions};

use resample::Resampler;
pub use resample::resize;

fn check_dimensions(width: u32, height: u32) -> Result<()> {
    ensure!(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= 64_000_000,
        "Invalid image dimensions or image exceeds 64 million pixels"
    );
    Ok(())
}

pub(crate) fn target_dimensions(
    width: u32,
    height: u32,
    frames: u32,
    scale: Scale,
) -> Result<(u32, u32)> {
    check_dimensions(width, height)?;
    ensure!(
        frames > 0 && width >= frames && height > 0,
        "Invalid texture frame layout: {width}x{height}, {frames} frames"
    );
    let width = scale
        .dimension(width / frames)
        .checked_mul(frames)
        .ok_or_else(|| anyhow::anyhow!("Texture dimensions overflow"))?;
    let height = scale.dimension(height);
    check_dimensions(width, height)?;
    Ok((width, height))
}

/// # Errors
/// Возвращает ошибку неподдерживаемого или повреждённого изображения.
pub fn decode(bytes: &[u8]) -> Result<RgbaImage> {
    if bytes.starts_with(b"DDS ") {
        let dds = image_dds::ddsfile::Dds::read(&mut std::io::Cursor::new(bytes))?;
        check_dimensions(dds.get_width(), dds.get_height())?;
        Ok(image_dds::image_from_dds(&dds, 0)?)
    } else {
        let decoder =
            image::ImageReader::with_format(std::io::Cursor::new(bytes), raster_format(bytes))
                .into_decoder()?;
        let (w, h) = decoder.dimensions();
        check_dimensions(w, h)?;
        Ok(DynamicImage::from_decoder(decoder)?.into_rgba8())
    }
}

#[must_use]
pub fn output_name(path: &str) -> String {
    let path = Path::new(path);
    path.with_file_name(format!(
        "{}_scaled.png",
        path.file_stem().unwrap_or_default().to_string_lossy()
    ))
    .to_string_lossy()
    .replace('\\', "/")
}

#[must_use]
pub fn is_resource_icon(path: &str) -> bool {
    path.starts_with("gfx/interface/icons/resources/")
        && Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dds"))
}

/// # Errors
/// Возвращает ошибку создания или записи несжатой DDS-текстуры.
pub fn resource_dds(image: &RgbaImage) -> Result<Vec<u8>> {
    use image_dds::ddsfile::{D3DFormat, Dds, NewD3dParams};

    let mut dds = Dds::new_d3d(NewD3dParams {
        height: image.height(),
        width: image.width(),
        depth: None,
        format: D3DFormat::A8R8G8B8,
        mipmap_levels: None,
        caps2: None,
    })?;
    // Старый загрузчик DDS игры ожидает BGRA; RGBA с масками A8B8G8R8 меняет красный и синий местами.
    for (output, pixel) in dds
        .data
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(image.pixels())
    {
        *output = [pixel[2], pixel[1], pixel[0], pixel[3]];
    }
    let mut bytes = Vec::new();
    dds.write(&mut bytes)?;
    Ok(bytes)
}

/// # Errors
/// Возвращает ошибку числа кадров или недопустимых размеров.
pub fn process(
    source: &RgbaImage,
    scale: Scale,
    frames: u32,
    options: &TextureOptions,
) -> Result<RgbaImage> {
    let (width, height) = target_dimensions(source.width(), source.height(), frames, scale)?;
    // Кадры имеют целую ширину; остаток справа не входит в используемую область атласа.
    let source = source.view(0, 0, source.width() / frames * frames, source.height());
    let parts = if options.independent_frames {
        frames
    } else {
        1
    };
    resize_frames(source, width, height, parts, options)
}

fn resize_frames(
    source: SubImage<&RgbaImage>,
    width: u32,
    height: u32,
    frames: u32,
    options: &TextureOptions,
) -> Result<RgbaImage> {
    let frame_width = source.width() / frames;
    let target_width = width / frames;
    let resampler = Resampler::new(
        (frame_width, source.height()),
        (target_width, height),
        options,
    )?;
    if frames == 1 {
        return Ok(resampler.resize(source));
    }
    let mut result = RgbaImage::new(width, height);
    for frame in 0..frames {
        let input = source.view(frame * frame_width, 0, frame_width, source.height());
        let output = resampler.resize(input);
        result.copy_from(&output, frame * target_width, 0)?;
    }
    Ok(result)
}

/// # Errors
/// Возвращает ошибку числа кадров или размеров фона.
pub fn compact_background(
    source: &RgbaImage,
    width: u32,
    height: u32,
    options: &TextureOptions,
) -> Result<RgbaImage> {
    check_dimensions(source.width(), source.height())?;
    ensure!(
        source.width().is_multiple_of(3),
        "Invalid row background frame count"
    );
    let total_width = width
        .checked_mul(3)
        .ok_or_else(|| anyhow::anyhow!("Texture dimensions overflow"))?;
    check_dimensions(total_width, height)?;
    resize_frames(
        source.view(0, 0, source.width(), source.height()),
        total_width,
        height,
        3,
        options,
    )
}

/// # Errors
/// Возвращает ошибку чтения заголовка изображения.
pub fn dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    if let Some(mut header) = bytes.strip_prefix(b"DDS ") {
        use image_dds::ddsfile::{FourCC, Header, Header10};

        // При сканировании читаем только заголовки, без копирования данных текстуры и mip-уровней.
        let dds = Header::read(&mut header)?;
        if dds.spf.fourcc == Some(FourCC(FourCC::DX10)) {
            Header10::read(&mut header)?;
        }
        Ok((dds.width, dds.height))
    } else {
        Ok(
            image::ImageReader::with_format(std::io::Cursor::new(bytes), raster_format(bytes))
                .into_dimensions()?,
        )
    }
}

/// # Errors
/// Возвращает ошибку размеров или раскладки кадров периодической текстуры.
pub fn periodic(
    source: &RgbaImage,
    scale: Scale,
    frames: u32,
    options: &TextureOptions,
) -> Result<RgbaImage> {
    let (width, target_height) = target_dimensions(source.width(), source.height(), frames, scale)?;
    let parts = if options.independent_frames {
        frames
    } else {
        1
    };
    let source_width = (source.width() / frames) * frames / parts;
    let target_width = width / parts;
    let padding = 8_u32;
    let scaled_padding = scale.dimension(padding);
    check_dimensions(source_width + padding * 2, source.height() + padding * 2)?;
    check_dimensions(
        target_width + scaled_padding * 2,
        target_height + scaled_padding * 2,
    )?;
    let mut output = RgbaImage::new(width, target_height);
    let mut options = *options;
    options.sharpening = 0.0;
    let resampler = Resampler::new(
        (source_width + 2 * padding, source.height() + 2 * padding),
        (
            target_width + 2 * scaled_padding,
            target_height + 2 * scaled_padding,
        ),
        &options,
    )?;
    for frame in 0..parts {
        let padded = RgbaImage::from_fn(
            source_width + padding * 2,
            source.height() + padding * 2,
            |x, y| {
                let x = (x + source_width * padding - padding) % source_width;
                let y = (y + source.height() * padding - padding) % source.height();
                *source.get_pixel(frame * source_width + x, y)
            },
        );
        let result = resampler.resize(padded.view(0, 0, padded.width(), padded.height()));
        let result = result.view(scaled_padding, scaled_padding, target_width, target_height);
        output.copy_from(&*result, frame * target_width, 0)?;
    }
    Ok(output)
}

fn raster_format(bytes: &[u8]) -> image::ImageFormat {
    // У TGA нет обязательной сигнатуры; при отсутствии известной сигнатуры проверяем его заголовок декодером.
    image::guess_format(bytes).unwrap_or(image::ImageFormat::Tga)
}
