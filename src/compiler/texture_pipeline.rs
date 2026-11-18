use std::{num::NonZeroUsize, path::Path};

use anyhow::Result;
use rayon::prelude::*;

use crate::{config::Profile, parallelism::CpuBudget, textures};

use super::{
    BuildPlan, Control, Progress, ProgressStage, TextureAsset, output, texture_cache::TextureCache,
};

// Оценка включает float-буферы фильтров, RGBA, PNG и DDS. Одна крупная текстура идёт отдельно.
const BATCH_BYTES: u64 = 256 * 1024 * 1024;

fn working_bytes(asset: &TextureAsset, profile: &Profile) -> Result<u64> {
    let scale = profile.texture_scale(&asset.path, asset.category);
    let (width, height) = asset.target_dimensions(scale)?;
    let source_padding = if asset.tiled { 16 } else { 0 };
    let target_padding = if asset.tiled {
        2 * scale.dimension(8)
    } else {
        0
    };
    let sw = u64::from(asset.width + source_padding);
    let sh = u64::from(asset.height + source_padding);
    let tw = u64::from(width + target_padding);
    let th = u64::from(height + target_padding);
    let mut pixels = (sw * sh).max(tw * sh).max(tw * th);
    if asset.options(profile).algorithm == crate::config::Algorithm::RealPlksr2x {
        // Нативный результат сети может быть больше конечного изображения при масштабе меньше двух.
        pixels = pixels.max(sw * sh * 4);
    }
    Ok(pixels * 64)
}

fn batch_len(assets: &[TextureAsset], profile: &Profile, concurrency: usize) -> Result<usize> {
    let mut count = 0;
    let mut bytes = 0;
    for asset in assets.iter().take(concurrency) {
        let estimate = working_bytes(asset, profile)?;
        if count > 0 && bytes + estimate > BATCH_BYTES {
            break;
        }
        count += 1;
        bytes += estimate;
    }
    Ok(count)
}

pub(super) fn compile(
    plan: &BuildPlan,
    profile: &Profile,
    root: &Path,
    cache: &mut TextureCache,
    control: &Control,
    progress: &mut dyn FnMut(Progress),
) -> Result<usize> {
    let budget = CpuBudget::current();
    let concurrency = budget.threads().min(plan.textures.len().max(1));
    // Ограничиваем также число рабочих потоков: их аллокаторы удерживают промежуточные буферы.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(concurrency)
        .build()?;
    let mut remaining = plan.textures.as_slice();
    let mut completed = 0;
    let mut cache_hits = 0;
    let mut processors = Vec::new();
    while !remaining.is_empty() {
        control.check()?;
        let count = batch_len(remaining, profile, concurrency)?;
        let (batch, rest) = remaining.split_at(count);
        let tasks = NonZeroUsize::new(count).unwrap_or(NonZeroUsize::MIN);
        let neural_threads = budget.per_task(tasks);
        // Бюджет CPU делится между активными задачами. Сессии сохраняются при неизменном бюджете; лишние освобождаются при уменьшении партии.
        processors.truncate(count);
        processors.resize_with(count, || {
            textures::Processor::with_neural_threads(neural_threads)
        });
        for processor in &mut processors {
            processor.set_neural_threads(neural_threads);
        }
        // Дисковые операции остаются последовательными; CPU обрабатывает ограниченную партию.
        let pending = batch
            .iter()
            .enumerate()
            .map(|(index, asset)| {
                control.check()?;
                progress(Progress {
                    stage: ProgressStage::Textures,
                    completed: completed + index,
                    total: plan.textures.len(),
                });
                control.check()?;
                cache.prepare(&plan.game, asset, profile)
            })
            .collect::<Result<Vec<_>>>()?;
        let results: Vec<_> = pool.install(|| {
            pending
                .into_par_iter()
                .zip(&processors)
                .map(|(texture, processor)| {
                    control.check()?;
                    texture.process(processor, control)
                })
                .collect()
        });
        for (asset, result) in batch.iter().zip(results) {
            control.check()?;
            let result = result?;
            result.store()?;
            output::write(root, &textures::output_name(&asset.path), &result.png)?;
            // Движок создаёт ресурсные спрайты из DDS по исходному пути, минуя ссылки GFX.
            if let Some(bytes) = result.resource_dds {
                output::write(root, &asset.path, &bytes)?;
            }
            cache_hits += usize::from(result.hit);
        }
        completed += count;
        remaining = rest;
    }
    Ok(cache_hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Category, Scale};

    #[test]
    fn batches_bound_concurrency_and_isolate_large_images() {
        let profile = Profile {
            scale: Scale::try_from(2.0).unwrap(),
            ..Default::default()
        };
        let asset = TextureAsset {
            path: "gfx/test.dds".into(),
            width: 64,
            height: 64,
            frames: 1,
            category: Category::Icon,
            tiled: false,
        };
        let mut assets = vec![asset; 5];
        assert_eq!(batch_len(&assets, &profile, 2).unwrap(), 2);
        assert_eq!(batch_len(&assets, &profile, 8).unwrap(), 5);
        assets[1].width = 2048;
        assets[1].height = 2048;
        assert_eq!(batch_len(&assets, &profile, 8).unwrap(), 1);
        assert_eq!(batch_len(&assets[1..], &profile, 8).unwrap(), 1);
        assets[0].frames = 0;
        assert!(batch_len(&assets, &profile, 8).is_err());
    }
}
