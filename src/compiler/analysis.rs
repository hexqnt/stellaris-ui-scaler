use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use crate::{
    assets::{self, Diagnostic, Installation},
    config::{AutoSourceMods, Category, Profile},
    pdx::{Document, Node},
    source_mods, textures,
};

use super::{
    BuildPlan, Control, EnabledModScan, Progress, ProgressStage, Sprite, SpriteSizing, TextureAsset,
};

const ARTWORK_PIXELS: u64 = 1_200_000;

struct TextureUsage {
    dimensions: Option<(u32, u32)>,
    frames: u32,
    tiled: bool,
    convert: bool,
    mask_only: bool,
}

#[derive(Default)]
struct Analysis {
    textures: BTreeMap<String, TextureUsage>,
    diagnostics: Vec<Diagnostic>,
}

impl Analysis {
    fn inspect_reference(
        &mut self,
        game: &Installation,
        reference: &str,
        path: &str,
        block: &Node<'_>,
    ) -> Result<Option<(u32, u32)>> {
        let (rule, message) = if game.contains(reference) {
            match textures::dimensions(&game.read(reference)?) {
                Ok(dimensions) => return Ok(Some(dimensions)),
                Err(error) => ("texture-unreadable", format!("{reference}: {error}")),
            }
        } else {
            ("source-missing", reference.to_owned())
        };
        self.diagnostics.push(Diagnostic {
            path: path.into(),
            span: Some(block.span.clone()),
            rule: rule.into(),
            message,
        });
        Ok(None)
    }

    fn sprite(
        &mut self,
        game: &Installation,
        path: &str,
        source: &str,
        block: &Node<'_>,
    ) -> Result<Sprite> {
        let tiled = block
            .key
            .is_some_and(|key| key.eq_ignore_ascii_case("corneredTileSpriteType"))
            || block.field("borderSize").is_some();
        let frames = block
            .field("noOfFrames")
            .and_then(Node::parse_atom::<u32>)
            .unwrap_or(1)
            .max(1);
        let mut references = Vec::new();
        block.try_walk(&mut |node| -> Result<()> {
            let Some(reference) = assets::texture_reference(node) else {
                return Ok(());
            };
            let mask = node.key.is_some_and(|key| {
                ["animationmaskfile", "masking_texture"]
                    .iter()
                    .any(|candidate| key.eq_ignore_ascii_case(candidate))
            });
            if !self.textures.contains_key(reference.as_ref()) {
                let dimensions = self.inspect_reference(game, &reference, path, block)?;
                self.textures.insert(
                    reference.to_string(),
                    TextureUsage {
                        dimensions,
                        frames: 1,
                        tiled: false,
                        convert: false,
                        mask_only: mask,
                    },
                );
            }
            if let Some(usage) = self.textures.get_mut(reference.as_ref()) {
                usage.tiled |= tiled;
                usage.mask_only &= mask;
            }
            references.push(reference.into_owned());
            Ok(())
        })?;
        let oversized = references.iter().any(|reference| {
            self.dimensions(reference).is_some_and(|(width, height)| {
                u64::from(width) * u64::from(height) > ARTWORK_PIXELS
            })
        });
        let render_target = block.key == Some("portraitType")
            || source.as_bytes()[block.span.clone()]
                .windows(b"rendertarget".len())
                .any(|window| window.eq_ignore_ascii_case(b"rendertarget"));
        let valid_primary =
            block
                .field("texturefile")
                .and_then(Node::atom)
                .is_none_or(|reference| {
                    self.dimensions(&assets::normalize_reference(reference))
                        .is_none_or(|(width, _)| width >= frames)
                });
        if !valid_primary {
            self.diagnostics.push(Diagnostic {
                path: path.into(),
                span: Some(block.span.clone()),
                rule: "source-frame-layout".into(),
                message: format!(
                    "{}: frame count exceeds texture width; sprite preserved",
                    block.named().unwrap_or_default()
                ),
            });
        }
        let convert = valid_primary
            && !tiled
            && !oversized
            && !render_target
            && references
                .iter()
                .any(|reference| self.dimensions(reference).is_some());
        if convert {
            for reference in &references {
                if let Some(usage) = self.textures.get_mut(reference) {
                    usage.convert = true;
                    if usage.dimensions.is_some_and(|(width, _)| width >= frames) {
                        usage.frames =
                            least_common_multiple(usage.frames, frames).with_context(|| {
                                format!("Combined frame count overflows: {reference}")
                            })?;
                    }
                }
            }
        }
        Ok(Sprite {
            file: path.into(),
            name: block.named().unwrap_or_default().into(),
            span: block.span.clone(),
            references,
            sprite_sheet: block
                .field("sprite_sheet_sprite_type")
                .and_then(Node::atom)
                .map(str::to_owned),
            tiled,
            convert,
            sizing: sprite_sizing(block, render_target),
        })
    }

    fn dimensions(&self, reference: &str) -> Option<(u32, u32)> {
        self.textures
            .get(reference)
            .and_then(|usage| usage.dimensions)
    }

    fn finish(mut self) -> (Vec<TextureAsset>, Vec<Diagnostic>) {
        let mut textures = Vec::new();
        for (path, usage) in self.textures {
            let Some((width, height)) = usage.dimensions else {
                continue;
            };
            let mut frames = usage.frames;
            if width < frames {
                self.diagnostics.push(Diagnostic {
                    path: path.clone(),
                    span: None,
                    rule: "frame-count".into(),
                    message: format!(
                        "Width {width} is smaller than {frames} frames; treating as one image"
                    ),
                });
                frames = 1;
            }
            if usage.convert {
                if !width.is_multiple_of(frames) {
                    self.diagnostics.push(Diagnostic {
                        path: path.clone(),
                        span: None,
                        rule: "frame-padding".into(),
                        message: format!(
                            "Omitting {} trailing pixels outside {frames} integer-width frames",
                            width % frames
                        ),
                    });
                }
                let category = if usage.mask_only {
                    Category::Mask
                } else {
                    classify(&path, width, height, frames, usage.tiled)
                };
                textures.push(TextureAsset {
                    path,
                    width,
                    height,
                    frames,
                    category,
                    tiled: usage.tiled,
                });
            }
        }
        (textures, self.diagnostics)
    }
}

/// # Errors
/// Возвращает ошибку чтения, разбора или отмены анализа.
pub fn plan(
    game: &Path,
    control: &Control,
    progress: &mut dyn FnMut(Progress),
) -> Result<BuildPlan> {
    plan_with_mods(game, &[], control, progress)
}

/// # Errors
/// Возвращает ошибку обнаружения модов или сканирования ресурсов.
pub fn plan_with_profile(
    game: &Path,
    profile: &Profile,
    control: &Control,
    progress: &mut dyn FnMut(Progress),
) -> Result<BuildPlan> {
    control.check()?;
    let sources = source_mods::resolve(game, profile)?;
    plan_with_mods(game, &sources, control, progress)
}

/// Загружает текущие включённые моды, сохраняя выбор из профиля и отключая новые источники.
/// # Errors
/// Возвращает ошибку обнаружения, чтения или отмены; исходный профиль не меняется.
pub fn scan_enabled_mods(
    game: &Path,
    profile: &Profile,
    control: &Control,
    progress: &mut dyn FnMut(Progress),
) -> Result<EnabledModScan> {
    control.check()?;
    profile.check()?;
    let user_data_dir = profile
        .auto_source_mods
        .as_ref()
        .and_then(|settings| settings.user_data_dir.clone());
    let discovered = source_mods::discover(game, user_data_dir.as_deref())?;
    let changes = profile.auto_source_mods.as_ref().and_then(|settings| {
        source_mods::ModListMismatch::between(&settings.source_mods, &discovered.source_mods)
    });
    let known: BTreeSet<_> = profile
        .auto_source_mods
        .iter()
        .flat_map(|settings| &settings.source_mods)
        .collect();
    let mut disabled_source_mods = profile.disabled_source_mods.clone();
    disabled_source_mods.extend(
        discovered
            .source_mods
            .iter()
            .filter(|path| !known.contains(path))
            .cloned(),
    );
    let selected: Vec<_> = discovered
        .source_mods
        .iter()
        .filter(|path| !disabled_source_mods.contains(*path))
        .cloned()
        .collect();
    let plan = plan_with_mods(game, &selected, control, progress)?;
    control.check()?;
    Ok(EnabledModScan {
        plan,
        settings: AutoSourceMods {
            user_data_dir,
            source_mods: discovered.source_mods,
        },
        changes,
        disabled_source_mods,
        removed_manual_sources: !profile.source_mods.is_empty(),
    })
}

/// # Errors
/// Возвращает ошибку чтения исходной игры, UI-модов, разбора или отмены анализа.
pub fn plan_with_mods(
    game: &Path,
    source_mods: &[PathBuf],
    control: &Control,
    progress: &mut dyn FnMut(Progress),
) -> Result<BuildPlan> {
    control.check()?;
    let game = Installation::open(game)?.with_source_mods(source_mods)?;
    let mut sources = BTreeMap::new();
    let mut sprites = Vec::new();
    let mut analysis = Analysis::default();
    analysis
        .diagnostics
        .extend(super::version::warning(&game.version));
    let paths: Vec<_> = game.interface_files().collect();
    for (index, path) in paths.iter().enumerate() {
        control.check()?;
        progress(Progress {
            stage: ProgressStage::Scan,
            completed: index,
            total: paths.len(),
        });
        control.check()?;
        let source = game.text(path)?;
        let doc = Document::parse(&source).with_context(|| format!("Cannot parse {path}"))?;
        if path.ends_with(".gfx") {
            doc.try_walk(|node| -> Result<()> {
                if node.named().is_some()
                    && node.key.is_some_and(|key| {
                        key.get(key.len().saturating_sub(4)..)
                            .is_some_and(|suffix| suffix.eq_ignore_ascii_case("type"))
                    })
                {
                    control.check()?;
                    sprites.push(analysis.sprite(&game, path, &source, node)?);
                }
                Ok(())
            })?;
        }
        sources.insert((*path).into(), source);
    }
    // Ресурсные значки, включая серые варианты, могут загружаться без объявления в GFX.
    for path in game
        .resources
        .keys()
        .filter(|path| textures::is_resource_icon(path))
    {
        control.check()?;
        if !analysis.textures.contains_key(path) {
            let dimensions = textures::dimensions(&game.read(path)?)
                .with_context(|| format!("Cannot inspect resource icon: {path}"))?;
            analysis.textures.insert(
                path.clone(),
                TextureUsage {
                    dimensions: Some(dimensions),
                    frames: 1,
                    tiled: false,
                    convert: true,
                    mask_only: false,
                },
            );
        }
    }
    control.check()?;
    let (textures, diagnostics) = analysis.finish();
    let fonts = crate::fonts::Catalog::load(&game, &sources)?;
    control.check()?;
    Ok(BuildPlan {
        fonts,
        game,
        sources,
        sprites,
        textures,
        diagnostics,
    })
}

fn sprite_sizing(block: &Node<'_>, render_target: bool) -> SpriteSizing {
    if render_target {
        SpriteSizing::RenderTarget
    } else if block
        .key
        .is_some_and(|key| key.eq_ignore_ascii_case("PieChartType"))
        && block
            .field("size")
            .and_then(Node::parse_atom::<f64>)
            .is_some_and(f64::is_finite)
    {
        SpriteSizing::Geometry
    } else {
        SpriteSizing::Texture
    }
}

fn classify(path: &str, width: u32, height: u32, frames: u32, tiled: bool) -> Category {
    if path.rsplit('/').next().is_some_and(|file| {
        file.split(['_', '-', '.', ' '])
            .any(|word| word.eq_ignore_ascii_case("mask"))
    }) {
        Category::Mask
    } else if frames > 1 {
        Category::SpriteSheet
    } else if tiled {
        Category::Frame
    } else if u64::from(width) * u64::from(height) > ARTWORK_PIXELS {
        Category::Artwork
    } else if width > 256 || height > 256 {
        Category::Background
    } else {
        Category::Icon
    }
}

const fn least_common_multiple(a: u32, b: u32) -> Option<u32> {
    if a == 0 || b == 0 {
        return Some(0);
    }
    let (mut x, mut y) = (a, b);
    while y != 0 {
        (x, y) = (y, x % y);
    }
    (a / x).checked_mul(b)
}

#[cfg(test)]
mod tests {
    use super::least_common_multiple;

    #[test]
    fn frame_counts_combine_without_overflow() {
        assert_eq!(least_common_multiple(4, 6), Some(12));
        assert_eq!(least_common_multiple(u32::MAX, u32::MAX), Some(u32::MAX));
        assert_eq!(least_common_multiple(65_536, 65_537), None);
        assert_eq!(least_common_multiple(0, 0), Some(0));
    }
}
