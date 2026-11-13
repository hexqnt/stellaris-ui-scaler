use anyhow::{Result, ensure};

use crate::{
    config::{Profile, Scale},
    textures,
};

use super::{BuildPlan, Control, TextureAsset};

pub struct TexturePreview {
    pub images: Vec<(String, image::RgbaImage)>,
    pub features: Option<crate::config::ImageFeatures>,
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
    )
}

pub(super) fn render_texture(
    asset: &TextureAsset,
    scale: Scale,
    source: &image::RgbaImage,
    options: &crate::config::TextureOptions,
) -> Result<image::RgbaImage> {
    ensure!(
        source.dimensions() == (asset.width, asset.height),
        "Source dimensions changed since scan: {}",
        asset.path
    );
    if asset.tiled {
        textures::periodic(source, scale, asset.frames, options)
    } else {
        textures::process(source, scale, asset.frames, options)
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
    profile.check()?;
    control.check()?;
    let source = textures::decode(&plan.game.read(&asset.path)?)?;
    let policy = asset.policy(profile);
    let needs_features = policy.needs_pixel_analysis()
        || algorithms
            .iter()
            .any(|&algorithm| policy.with_algorithm(algorithm).needs_pixel_analysis());
    let features = needs_features
        .then(|| crate::config::ImageFeatures::analyze(&source, asset.frames))
        .transpose()?;
    let mut output = Vec::with_capacity(algorithms.len() + 1);
    for (index, algorithm) in algorithms.iter().enumerate() {
        control.check()?;
        let options = policy
            .with_algorithm(*algorithm)
            .processing(features.as_ref())
            .options;
        output.push((
            format!("{}: {algorithm:?}", index + 1),
            render_texture(asset, policy.scale, &source, &options)?,
        ));
    }
    output.insert(0, ("Original".into(), source));
    Ok(TexturePreview {
        images: output,
        features,
    })
}
