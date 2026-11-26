mod layout;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use anyhow::{Context, Result};

use crate::{
    assets::{self, Diagnostic},
    config::{Category, OUTLINER_REFERENCE_SCALE, Profile},
    fonts, outliner,
    pdx::{Document, Edits, Node},
    scaling, textures,
};

use super::output::write;
use super::{BuildPlan, Control, Progress, ProgressStage, Sprite, SpriteSizing, png};

pub(super) fn compile(
    plan: &BuildPlan,
    profile: &Profile,
    root: &Path,
    converted: &BTreeSet<&str>,
    control: &Control,
    progress: &mut dyn FnMut(Progress),
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let statuses = sprite_statuses(&plan.sprites, converted);
    let mut eligible = BTreeMap::<&str, BTreeSet<usize>>::new();
    for sprite in plan.sprites.iter().filter(|sprite| sprite.convert) {
        eligible
            .entry(&sprite.file)
            .or_default()
            .insert(sprite.span.start);
    }
    let empty = BTreeSet::new();
    let mut shared_background = None;
    for (index, (path, source)) in plan.sources.iter().enumerate() {
        control.check()?;
        progress(Progress::new(
            ProgressStage::Interface,
            index,
            plan.sources.len(),
        ));
        let mut text = if path.ends_with(".gui") {
            transform_gui(source, path, profile, &statuses, diagnostics)?
        } else if fonts::is_font_file(path) {
            fonts::compile(
                source,
                &plan.game,
                profile,
                &plan.fonts,
                &mut |path, bytes| write(root, path, bytes),
            )?
        } else {
            transform_gfx(
                source,
                path,
                eligible.get(path.as_str()).unwrap_or(&empty),
                profile,
                converted,
                diagnostics,
            )?
        };
        if path == "interface/outliner.gfx" && profile.outliner.compact {
            text = patch_outliner(&text, profile, &mut shared_background)?;
        }
        if text != *source {
            Document::parse(&text).with_context(|| format!("Invalid generated syntax: {path}"))?;
            write(root, path, text.as_bytes())?;
        }
    }
    write_support(plan, profile, root, shared_background)
}

fn sprite_statuses<'a>(
    sprites: &'a [Sprite],
    converted: &BTreeSet<&str>,
) -> BTreeMap<&'a str, bool> {
    let mut statuses: BTreeMap<_, _> = sprites
        .iter()
        .map(|sprite| {
            (
                sprite.name.as_str(),
                match sprite.sizing {
                    SpriteSizing::Geometry => true,
                    SpriteSizing::RenderTarget => false,
                    SpriteSizing::Texture => {
                        sprite.convert
                            && sprite
                                .references
                                .iter()
                                .any(|path| converted.contains(path.as_str()))
                    }
                },
            )
        })
        .collect();
    // Ссылки могут идти цепочкой и объявляться после использующего их спрайта.
    // Статус меняется только на true, поэтому циклы без текстур завершаются сразу.
    loop {
        let mut changed = false;
        for sprite in sprites {
            if sprite.sizing != SpriteSizing::RenderTarget
                && let Some(sheet) = &sprite.sprite_sheet
                && statuses.get(sheet.as_str()) == Some(&true)
                && statuses.get(sprite.name.as_str()) == Some(&false)
            {
                statuses.insert(sprite.name.as_str(), true);
                changed = true;
            }
        }
        if !changed {
            return statuses;
        }
    }
}

fn transform_gui(
    source: &str,
    path: &str,
    profile: &Profile,
    statuses: &BTreeMap<&str, bool>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<String> {
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    scaling::geometry_edits(&doc, profile.scale, false, path, &mut edits, diagnostics);
    scaling::widget_edits(
        &doc,
        profile.scale,
        |sprite| statuses.get(sprite) == Some(&false),
        &mut edits,
    );
    let text = edits.apply(source)?;
    let text = match path {
        "interface/main.gui" => layout::navbar(&text, profile.scale)?,
        "interface/assign_trait_view.gui" => layout::trait_buttons(&text, profile.scale)?,
        _ => text,
    };
    if !profile.outliner.compact {
        return Ok(text);
    }
    let text = outliner::shared_gui(&text, path, profile.scale, &profile.outliner, diagnostics)?;
    if path == "interface/outliner.gui" {
        outliner::transform(&text, &profile.outliner, profile.scale)
    } else {
        Ok(text)
    }
}

fn transform_gfx(
    source: &str,
    path: &str,
    eligible: &BTreeSet<usize>,
    profile: &Profile,
    converted: &BTreeSet<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<String> {
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    doc.walk(|node| {
        if eligible.contains(&node.span.start) {
            rewrite_references(node, converted, &mut edits);
        }
        if node.named() == Some("GFX_topbar_background_cutoff")
            && let Some(border) = node.field("borderSize")
        {
            for axis in ["x", "y"] {
                if let Some(value) = border.field(axis) {
                    scaling::scaled_atom(value, profile.scale, &mut edits);
                }
            }
        }
    });
    // Диапазоны диагностики выражений относятся к тексту после замены ссылок.
    if source.contains("@[") {
        return scaling::geometry(
            &edits.apply(source)?,
            profile.scale,
            true,
            path,
            diagnostics,
        );
    }
    scaling::geometry_edits(&doc, profile.scale, true, path, &mut edits, diagnostics);
    edits.apply(source)
}

fn rewrite_references(node: &Node<'_>, converted: &BTreeSet<&str>, edits: &mut Edits) {
    node.walk(&mut |child| {
        if let Some((normalized, _)) = assets::texture_reference(child)
            && converted.contains(normalized.as_ref())
        {
            edits.atom(child, format!("\"{}\"", textures::output_name(&normalized)));
        }
    });
}

fn patch_outliner(
    source: &str,
    profile: &Profile,
    shared_background: &mut Option<String>,
) -> Result<String> {
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    doc.walk(|n| {
        if n.named() == Some("GFX_outliner_member_bg") {
            *shared_background = Some(source[n.span.clone()].replacen(
                "GFX_outliner_member_bg",
                outliner::SHARED_BACKGROUND,
                1,
            ));
        }
        let target = match n.named() {
            Some("GFX_outliner_member_bg") => Some(outliner::ROW_BACKGROUND),
            Some("GFX_outliner_member_bg_selected") => Some(outliner::SELECTED_BACKGROUND),
            _ => None,
        };
        if let Some(target) = target
            && let Some(field) = n.field("textureFile")
        {
            edits.atom(field, format!("\"{target}\""));
        }
        if n.named().is_some_and(|s| {
            [
                "GFX_outliner_planet_colonize_progressbar",
                "GFX_outliner_fleet_progressbar",
                "GFX_outliner_arkship_progressbar",
                "GFX_outliner_planet_terraforming_progressbar",
            ]
            .contains(&s)
        }) && let Some(w) = n.field("size").and_then(|s| s.field("x"))
        {
            edits.atom(
                w,
                format!(
                    "{:.0}",
                    (225.0 * profile.scale.get() / OUTLINER_REFERENCE_SCALE).round_ties_even()
                ),
            );
        }
    });
    edits.apply(source)
}

fn write_support(
    plan: &BuildPlan,
    profile: &Profile,
    root: &Path,
    shared_background: Option<String>,
) -> Result<()> {
    // borderSize задаёт и размер углов, и границы выборки в пикселях DDS.
    // Увеличиваем текстуру вместе с рамкой, чтобы видимая высота угла росла с панелью.
    // CorneredTile оставляем в DDS: загрузка PNG этим типом спрайта не подтверждена.
    let music_background = "gfx/interface/main/topbar_background_cutoff.dds";
    if plan.game.contains(music_background) {
        let source = textures::decode(&plan.game.read(music_background)?)?;
        let policy = profile.texture_policy(music_background, Category::Frame, None);
        let image = textures::process(&source, policy.scale, 1, &policy.processing(None).options)?;
        write(root, music_background, &textures::resource_dds(&image)?)?;
    }
    if profile.outliner.compact {
        let shared = shared_background.context("Missing shared outliner background sprite")?;
        write(
            root,
            "interface/ui_scaler_member_bg.gfx",
            format!("spriteTypes = {{\n{shared}\n}}\n").as_bytes(),
        )?;
        let (width, height, _) = outliner::dimensions(&profile.outliner, profile.scale);
        for (source, target) in [
            (
                "gfx/interface/buttons/outliner_entry.dds",
                outliner::ROW_BACKGROUND,
            ),
            (
                "gfx/interface/buttons/outliner_selected_entry.dds",
                outliner::SELECTED_BACKGROUND,
            ),
        ] {
            let image = textures::decode(&plan.game.read(source)?)?;
            let result = textures::compact_background(&image, width, height, &profile.textures)?;
            write(root, target, &png(&result)?)?;
        }
    }
    for path in plan
        .game
        .resources
        .keys()
        .filter(|p| p.starts_with("unchecked_defines/") && p.ends_with(".txt"))
    {
        let source = plan.game.text(path)?;
        let result = scaling::defines(&source, profile.scale)?;
        if result != source {
            write(root, path, result.as_bytes())?;
        }
    }
    if plan.game.contains("fonts/fonts.asset") {
        write(
            root,
            "fonts/fonts.asset",
            fonts::definitions(&plan.game.text("fonts/fonts.asset")?)?.as_bytes(),
        )?;
    }
    Ok(())
}
