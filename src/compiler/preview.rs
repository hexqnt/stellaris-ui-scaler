use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex, mpsc},
};

use rayon::prelude::*;

use anyhow::{Context, Result, ensure};

use crate::{
    config::{Profile, Scale},
    parallelism::CpuBudget,
    textures,
};

use super::{BuildPlan, Control, TextureAsset};

/// Оригинал и независимые результаты кандидатов с индексами исходного списка.
pub enum PreviewEvent {
    Source {
        image: Arc<image::RgbaImage>,
        features: Option<crate::config::ImageFeatures>,
    },
    Candidate {
        index: usize,
        image: Result<image::RgbaImage>,
    },
}

pub struct TexturePreview {
    pub images: Vec<(String, image::RgbaImage)>,
    pub features: Option<crate::config::ImageFeatures>,
}

/// Сохраняет нейросетевую сессию между запросами предпросмотра.
#[derive(Default)]
pub struct PreviewSession {
    processor: Mutex<textures::Processor>,
}

impl PreviewSession {
    /// Отправляет оригинал до запуска кандидатов, затем каждый результат по готовности. Обработчик результатов вызывается параллельно из рабочих потоков Rayon. Запросы одной сессии обрабатываются по очереди.
    ///
    /// # Errors
    /// Возвращает ошибку подготовки исходника или отмены; ошибки кандидатов передаются событиями.
    pub fn render(
        &self,
        plan: &BuildPlan,
        profile: &Profile,
        asset: &TextureAsset,
        algorithms: &[crate::config::Algorithm],
        control: &Control,
        emit: &(impl Fn(PreviewEvent) + Sync),
    ) -> Result<()> {
        profile.check()?;
        control.check()?;
        let source = Arc::new(textures::decode(&plan.game.read(&asset.path)?)?);
        let policy = asset.policy(profile);
        let needs_features = policy.needs_pixel_analysis()
            || algorithms
                .iter()
                .any(|&algorithm| policy.with_algorithm(algorithm).needs_pixel_analysis());
        let features = needs_features
            .then(|| crate::config::ImageFeatures::analyze(&source, asset.frames))
            .transpose()?;
        control.check()?;
        emit(PreviewEvent::Source {
            image: Arc::clone(&source),
            features,
        });
        if algorithms.is_empty() {
            return control.check();
        }
        let budget = CpuBudget::current();
        let tasks =
            NonZeroUsize::new(algorithms.len().min(budget.threads())).unwrap_or(NonZeroUsize::MIN);
        let mut session = self
            .processor
            .lock()
            .map_err(|_| anyhow::anyhow!("Preview session lock poisoned"))?;
        session.set_neural_threads(budget.per_task(tasks));
        let processor = &*session;
        algorithms
            .par_iter()
            .enumerate()
            .for_each(|(index, &algorithm)| {
                if control.check().is_err() {
                    return;
                }
                let options = policy
                    .with_algorithm(algorithm)
                    .processing(features.as_ref())
                    .options;
                let image =
                    render_texture(asset, policy.scale, &source, &options, processor, control);
                if control.check().is_ok() {
                    emit(PreviewEvent::Candidate { index, image });
                }
            });
        drop(session);
        control.check()
    }
}
/// # Errors
/// Возвращает ошибку профиля, чтения, декодирования или обработки изображения.
pub fn preview(
    plan: &BuildPlan,
    profile: &Profile,
    asset: &TextureAsset,
) -> Result<image::RgbaImage> {
    profile.check()?;
    let source = textures::decode(&plan.game.read(&asset.path)?)?;
    let policy = asset.policy(profile);
    let features = policy.analyze(&source, asset.frames)?;
    render_texture(
        asset,
        policy.scale,
        &source,
        &policy.processing(features.as_ref()).options,
        &textures::Processor::default(),
        &Control::default(),
    )
}

pub(super) fn render_texture(
    asset: &TextureAsset,
    scale: Scale,
    source: &image::RgbaImage,
    options: &crate::config::TextureOptions,
    processor: &textures::Processor,
    control: &Control,
) -> Result<image::RgbaImage> {
    ensure!(
        source.dimensions() == (asset.width, asset.height),
        "Source dimensions changed since scan: {}",
        asset.path
    );
    if asset.tiled {
        processor.periodic(source, scale, asset.frames, options, &|| control.check())
    } else {
        processor.process(source, scale, asset.frames, options, &|| control.check())
    }
}

/// # Errors
/// Возвращает ошибку чтения, обработки или отмены предпросмотра.
pub fn preview_candidates(
    plan: &BuildPlan,
    profile: &Profile,
    asset: &TextureAsset,
    algorithms: &[crate::config::Algorithm],
    control: &Control,
) -> Result<Vec<(String, image::RgbaImage)>> {
    Ok(preview_candidates_with_analysis(plan, profile, asset, algorithms, control)?.images)
}

/// # Errors
/// Возвращает ошибку профиля, анализа исходника, обработки или отмены предпросмотра.
pub fn preview_candidates_with_analysis(
    plan: &BuildPlan,
    profile: &Profile,
    asset: &TextureAsset,
    algorithms: &[crate::config::Algorithm],
    control: &Control,
) -> Result<TexturePreview> {
    let (sender, receiver) = mpsc::channel();
    PreviewSession::default().render(plan, profile, asset, algorithms, control, &|event| {
        let _ = sender.send(event);
    })?;
    drop(sender);
    let mut original = None;
    let mut features = None;
    let mut candidates = Vec::with_capacity(algorithms.len());
    for event in receiver {
        match event {
            PreviewEvent::Source {
                image,
                features: analysis,
            } => {
                original = Some(
                    Arc::try_unwrap(image)
                        .map_err(|_| anyhow::anyhow!("Preview source is still in use"))?,
                );
                features = analysis;
            }
            PreviewEvent::Candidate { index, image } => candidates.push((index, image)),
        }
    }
    // События приходят из рабочих потоков; порядок восстанавливается по индексу.
    candidates.sort_unstable_by_key(|(index, _)| *index);
    let mut images = Vec::with_capacity(algorithms.len() + 1);
    images.push((
        "Original".into(),
        original.context("Preview source was not emitted")?,
    ));
    for (index, image) in candidates {
        images.push((
            format!("{}: {}", index + 1, algorithms[index].name()),
            image?,
        ));
    }
    Ok(TexturePreview { images, features })
}
