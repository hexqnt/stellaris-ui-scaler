use std::{num::NonZeroUsize, path::Path};

use anyhow::{Context, Result};
use rayon::prelude::*;

use crate::{
    config::{Profile, TexturePolicy},
    parallelism::CpuBudget,
    textures,
};

use super::{
    BuildPlan, Control, Progress, ProgressStage, TextureAsset, output, texture_cache::TextureCache,
};

// Оценка включает float-буферы фильтров, RGBA, PNG и DDS. Одна крупная текстура идёт отдельно.
const BATCH_BYTES: u64 = 256 * 1024 * 1024;
// Ограничиваем копии весов и рабочие арены ONNX; бюджет буферов изображений считается отдельно.
const MAX_NEURAL_TASKS: usize = 4;

fn working_bytes(asset: &TextureAsset, policy: TexturePolicy) -> Result<u64> {
    let scale = policy.scale;
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
    if policy.uses_neural() {
        // Нативный результат сети может быть больше конечного изображения при масштабе меньше двух.
        pixels = pixels.max(sw * sh * 4);
    }
    Ok(pixels * 64)
}

fn batch_len(
    assets: &[TextureAsset],
    profile: &Profile,
    cpu_threads: usize,
    neural_threads: NonZeroUsize,
) -> Result<usize> {
    let mut count = 0;
    let mut bytes = 0;
    let mut used_threads = 0;
    let mut neural_tasks = 0;
    for asset in assets.iter().take(cpu_threads) {
        let policy = asset.policy(profile);
        let estimate = working_bytes(asset, policy)?;
        let neural = policy.uses_neural();
        let required_threads = if neural { neural_threads.get() } else { 1 };
        if count > 0
            && (bytes + estimate > BATCH_BYTES
                || used_threads + required_threads > cpu_threads
                || (neural && neural_tasks == MAX_NEURAL_TASKS))
        {
            break;
        }
        count += 1;
        bytes += estimate;
        used_threads += required_threads;
        neural_tasks += usize::from(neural);
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
    if plan.textures.is_empty() {
        return Ok(0);
    }
    let budget = CpuBudget::current();
    let worker_count = budget.threads().min(plan.textures.len());
    let neural_count = plan
        .textures
        .iter()
        .filter(|asset| asset.policy(profile).uses_neural())
        .take(MAX_NEURAL_TASKS.min(worker_count))
        .count();
    let neural_threads =
        budget.per_task(NonZeroUsize::new(neural_count).unwrap_or(NonZeroUsize::MIN));
    // Постоянный бюджет сохраняет сессии между партиями разного размера.
    let neural_processors: Vec<_> = (0..neural_count)
        .map(|_| textures::Processor::with_neural_threads(neural_threads))
        .collect();
    let regular = textures::Processor::with_neural_threads(NonZeroUsize::MIN);
    // Ограничиваем также число рабочих потоков: их аллокаторы удерживают промежуточные буферы.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(worker_count)
        .build()?;
    let mut remaining = plan.textures.as_slice();
    let mut completed = 0;
    let mut cache_hits = 0;
    while !remaining.is_empty() {
        control.check()?;
        let count = batch_len(remaining, profile, budget.threads(), neural_threads)?;
        let (batch, rest) = remaining.split_at(count);
        let mut neural = neural_processors.iter();
        // Чтение последовательно; готовые результаты записываются рабочими потоками сразу.
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
                let texture = cache.prepare(&plan.game, asset, profile)?;
                let processor = if texture.uses_neural() {
                    neural.next().context("Batch exceeds neural worker limit")?
                } else {
                    &regular
                };
                Ok((asset, texture, processor))
            })
            .collect::<Result<Vec<_>>>()?;
        cache_hits += pool.install(|| {
            pending
                .into_par_iter()
                .map(|(asset, texture, processor)| -> Result<usize> {
                    control.check()?;
                    let result = texture.process(processor, control)?;
                    control.check()?;
                    result.store()?;
                    output::write(root, &textures::output_name(&asset.path), &result.png)?;
                    // Движок загружает ресурсные значки по исходному пути DDS.
                    if let Some(bytes) = result.resource_dds {
                        output::write(root, &asset.path, &bytes)?;
                    }
                    Ok(usize::from(result.hit))
                })
                .try_reduce(|| 0, |a, b| Ok(a + b))
        })?;
        completed += count;
        remaining = rest;
    }
    Ok(cache_hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Algorithm, Category, Scale};

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
        let threads = NonZeroUsize::MIN;
        assert_eq!(batch_len(&assets, &profile, 2, threads).unwrap(), 2);
        assert_eq!(batch_len(&assets, &profile, 8, threads).unwrap(), 5);
        assets[1].width = 2048;
        assets[1].height = 2048;
        assert_eq!(batch_len(&assets, &profile, 8, threads).unwrap(), 1);
        assert_eq!(batch_len(&assets[1..], &profile, 8, threads).unwrap(), 1);
        assets[0].frames = 0;
        assert!(batch_len(&assets, &profile, 8, threads).is_err());
    }

    #[test]
    fn mixed_batches_respect_cpu_and_neural_session_limits() {
        let mut profile = Profile {
            scale: Scale::try_from(2.0).unwrap(),
            ..Default::default()
        };
        profile.textures.algorithm = Algorithm::RealPlksr2x;
        let asset = TextureAsset {
            path: "gfx/neural.png".into(),
            width: 16,
            height: 16,
            frames: 1,
            category: Category::Icon,
            tiled: false,
        };
        let mut assets = vec![asset; 6];
        assert_eq!(
            batch_len(&assets, &profile, 8, NonZeroUsize::MIN).unwrap(),
            4
        );
        let threads = NonZeroUsize::new(2).unwrap();
        assert_eq!(batch_len(&assets, &profile, 6, threads).unwrap(), 3);
        // Один обычный фильтр оставляет место только двум нейросетевым задачам по два CPU.
        profile.assets.insert(
            "gfx/regular.png".into(),
            crate::config::TextureRule {
                algorithm: Some(Algorithm::Lanczos3),
                ..Default::default()
            },
        );
        assets[0].path = "gfx/regular.png".into();
        assert_eq!(batch_len(&assets, &profile, 6, threads).unwrap(), 3);
        profile.scale = Scale::try_from(0.5).unwrap();
        assert_eq!(batch_len(&assets, &profile, 6, threads).unwrap(), 6);
    }
}
