// Каналы насыщаются при кодировании; размеры предварительно ограничены лимитом изображения.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::{collections::VecDeque, fs, num::NonZeroUsize, path::PathBuf, sync::Mutex};

use anyhow::{Context, Result, ensure};
use image::{GenericImageView, RgbaImage, SubImage, imageops};
use ort::{environment::Environment, session::Session, value::TensorRef};
use sha2::{Digest, Sha256};

use super::{Boundary, check_dimensions};

pub(super) const MODEL_SHA256: [u8; 32] = [
    0x8f, 0xb9, 0x23, 0xd0, 0xb1, 0xad, 0x09, 0xd4, 0x95, 0xc7, 0xb9, 0x7c, 0x60, 0x46, 0x27, 0x7c,
    0x6f, 0x0c, 0x6c, 0x8d, 0x59, 0xf5, 0xa9, 0xc8, 0x6e, 0x4b, 0xab, 0x70, 0x74, 0x45, 0xf2, 0x0f,
];
pub(super) const CACHE_ID: &str = "realplksr-2x-mssim-ort1.32-fp32-cpu-core192-halo32-v2";
const CORE: u32 = 192;
const HALO: u32 = 32;
static ENVIRONMENT: Mutex<Option<Environment>> = Mutex::new(None);

/// Сессия и входной буфер принадлежат одной задаче сборки или группе предпросмотров.
pub(super) struct Model {
    session: Session,
    input: Vec<f32>,
}

impl Model {
    pub(super) fn load(threads: NonZeroUsize) -> Result<Self> {
        let root = resource_directory()?;
        Self::load_from(&root, threads)
    }

    fn load_from(root: &std::path::Path, threads: NonZeroUsize) -> Result<Self> {
        let path = root.join("realplksr-2x.onnx");
        let bytes = fs::read(&path)
            .with_context(|| format!("Cannot read RealPLKSR model: {}", path.display()))?;
        ensure!(
            Sha256::digest(&bytes).as_slice() == MODEL_SHA256,
            "RealPLKSR model checksum mismatch; run tools/prepare_ai.py"
        );
        let library = root.join(if cfg!(windows) {
            "onnxruntime.dll"
        } else {
            "libonnxruntime.so"
        });
        let environment = {
            // ONNX Runtime допускает одно окружение на процесс, но отдельные сессии для предпросмотров и сборок.
            let mut environment = ENVIRONMENT
                .lock()
                .map_err(|_| anyhow::anyhow!("ONNX environment lock poisoned"))?;
            if environment.is_none() {
                *environment = Some(
                    ort::init_from(&library)
                        .with_context(|| {
                            format!("Cannot load ONNX Runtime: {}", library.display())
                        })?
                        .with_name("stellaris-ui-scaler")
                        .with_telemetry(false)
                        .build()?,
                );
            }
            environment
                .as_ref()
                .context("Missing ONNX environment")?
                .clone()
        };
        let session = Session::builder(&environment)?
            .with_intra_threads(threads.get())
            .map_err(ort::Error::<()>::from)?
            .with_inter_threads(1)
            .map_err(ort::Error::<()>::from)?
            .with_memory_pattern(false)
            .map_err(ort::Error::<()>::from)?
            .commit_from_memory(&bytes)?;
        Ok(Self {
            session,
            input: Vec::new(),
        })
    }

    pub(super) fn upscale(
        &mut self,
        source: SubImage<&RgbaImage>,
        boundary: Boundary,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<RgbaImage> {
        let (width, height) = source.dimensions();
        let target = (width * 2, height * 2);
        check_dimensions(target.0, target.1)?;
        let mut result = if source.pixels().any(|(_, _, pixel)| pixel[3] != 255) {
            imageops::resize(&*source, target.0, target.1, imageops::FilterType::Lanczos3)
        } else {
            RgbaImage::from_pixel(target.0, target.1, image::Rgba([0, 0, 0, 255]))
        };
        if source.pixels().all(|(_, _, pixel)| pixel[3] == 0) {
            result.fill(0);
            return Ok(result);
        }
        // RGB прозрачных пикселей не должен менять видимые края после свёрток.
        let filled = source
            .pixels()
            .any(|(_, _, pixel)| pixel[3] == 0)
            .then(|| extend_transparent(source));
        let input_source = filled.as_ref().map_or(source, |image| {
            image.view(0, 0, image.width(), image.height())
        });
        for ty in TileAxis::tiles(height, boundary) {
            for tx in TileAxis::tiles(width, boundary) {
                check()?;
                self.tile(input_source, tx, ty, &mut result)?;
            }
        }
        check()?;
        Ok(result)
    }

    fn tile(
        &mut self,
        source: SubImage<&RgbaImage>,
        tx: TileAxis,
        ty: TileAxis,
        result: &mut RgbaImage,
    ) -> Result<()> {
        let area = tx.input_len as usize * ty.input_len as usize;
        self.input.resize(area * 3, 0.0);
        // Координаты с оборачиванием вычисляются один раз на столбец, а не для каждого канала и пикселя.
        let mut columns = [0; (CORE + 2 * HALO) as usize];
        for (x, sx) in columns[..tx.input_len as usize].iter_mut().enumerate() {
            *sx = tx.sample(x, source.width());
        }
        let columns = &columns[..tx.input_len as usize];
        for (channel, plane) in self.input.chunks_exact_mut(area).enumerate() {
            for (y, row) in plane.chunks_exact_mut(tx.input_len as usize).enumerate() {
                let sy = ty.sample(y, source.height());
                for (value, &sx) in row.iter_mut().zip(columns) {
                    *value = f32::from(source.get_pixel(sx, sy)[channel]) / 255.0;
                }
            }
        }
        let input = TensorRef::from_array_view((
            [1, 3, ty.input_len as usize, tx.input_len as usize],
            self.input.as_slice(),
        ))?;
        let outputs = self.session.run(ort::inputs![input])?;
        let (shape, pixels) = outputs[0].try_extract_tensor::<f32>()?;
        let ow = tx.input_len as usize * 2;
        let oh = ty.input_len as usize * 2;
        ensure!(
            shape.as_ref()
                == [
                    1,
                    3,
                    i64::from(ty.input_len) * 2,
                    i64::from(tx.input_len) * 2
                ],
            "Unexpected RealPLKSR output shape: {shape:?}"
        );
        ensure!(
            pixels.iter().all(|value| value.is_finite()),
            "RealPLKSR produced non-finite pixels"
        );
        let mut planes = pixels.chunks_exact(ow * oh);
        let red = planes.next().context("Missing RealPLKSR red channel")?;
        let green = planes.next().context("Missing RealPLKSR green channel")?;
        let blue = planes.next().context("Missing RealPLKSR blue channel")?;
        let start_x = tx.crop as usize * 2;
        let start_y = ty.crop as usize * 2;
        let stride = result.width() as usize * 4;
        for (y, row) in result
            .as_mut()
            .chunks_exact_mut(stride)
            .skip(ty.start as usize * 2)
            .take(ty.core_len as usize * 2)
            .enumerate()
        {
            let start = (start_y + y) * ow + start_x;
            let range = start..start + tx.core_len as usize * 2;
            let target = &mut row[tx.start as usize * 8..(tx.start + tx.core_len) as usize * 8];
            for (((pixel, r), g), b) in target
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(&red[range.clone()])
                .zip(&green[range.clone()])
                .zip(&blue[range])
            {
                pixel[..3].copy_from_slice(
                    &[*r, *g, *b].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8),
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct TileAxis {
    start: u32,
    core_len: u32,
    input_start: i64,
    input_len: u32,
    crop: u32,
}

impl TileAxis {
    fn tiles(length: u32, boundary: Boundary) -> impl Iterator<Item = Self> {
        let count = length.div_ceil(CORE);
        (0..count).map(move |index| {
            // Равные части не оставляют узкую последнюю плитку с другой статистикой нормализации.
            let start = (u64::from(index) * u64::from(length) / u64::from(count)) as u32;
            let end = (u64::from(index + 1) * u64::from(length) / u64::from(count)) as u32;
            Self::new(start, end, length, boundary)
        })
    }

    fn new(start: u32, end: u32, length: u32, boundary: Boundary) -> Self {
        let core_len = end - start;
        let (before, after) = match boundary {
            Boundary::Image => (HALO.min(start), HALO.min(length - start - core_len)),
            Boundary::Periodic => (HALO, HALO),
        };
        Self {
            start,
            core_len,
            input_start: i64::from(start) - i64::from(before),
            input_len: core_len + before + after,
            crop: before,
        }
    }

    fn sample(self, offset: usize, length: u32) -> u32 {
        (self.input_start + i64::from(offset as u32)).rem_euclid(i64::from(length)) as u32
    }
}

fn resource_directory() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("STELLARIS_UI_SCALER_AI_DIR") {
        return Ok(PathBuf::from(path));
    }
    let executable = std::env::current_exe()?;
    let beside_executable = executable
        .parent()
        .context("Missing executable directory")?
        .join("ai");
    if beside_executable.is_dir() {
        return Ok(beside_executable);
    }
    let local = std::env::current_dir()?.join("ai");
    ensure!(
        local.is_dir(),
        "RealPLKSR 2x resources are missing. Run tools/prepare_ai.py, place the ai directory beside the executable, or set STELLARIS_UI_SCALER_AI_DIR"
    );
    Ok(local)
}

fn extend_transparent(source: SubImage<&RgbaImage>) -> RgbaImage {
    let mut filled = source.to_image();
    let width = filled.width() as usize;
    let height = filled.height() as usize;
    let pixels = filled.as_mut().as_chunks_mut::<4>().0;
    let mut queue = VecDeque::new();
    for index in 0..pixels.len() {
        if pixels[index][3] > 0 && neighbors(index, width, height).any(|next| pixels[next][3] == 0)
        {
            queue.push_back(index);
        }
    }
    while let Some(index) = queue.pop_front() {
        let mut color = pixels[index];
        color[3] = 255;
        for next in neighbors(index, width, height) {
            if pixels[next][3] == 0 {
                pixels[next] = color;
                queue.push_back(next);
            }
        }
    }
    filled
}

fn neighbors(index: usize, width: usize, height: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (index % width, index / width);
    [
        (x > 0).then(|| index - 1),
        (x + 1 < width).then_some(index + 1),
        (y > 0).then(|| index - width),
        (y + 1 < height).then_some(index + width),
    ]
    .into_iter()
    .flatten()
}

#[cfg(test)]
mod tests;
