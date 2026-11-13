use std::{collections::BTreeMap, path::Path};

use anyhow::{Context, Result, ensure};

use crate::{
    assets::Installation,
    config::Profile,
    pdx::{Document, Edits, Node},
    textures,
};

use super::{Catalog, bitmap_paths, scaled_size};

struct Record<'a> {
    kind: &'a str,
    doc: Document<'a>,
}

/// Разобранный одностраничный текстовый атлас с известным размером шрифта.
pub struct Font<'a> {
    records: Vec<Record<'a>>,
    pub size: f64,
    pub dimensions: (u32, u32),
}

impl<'a> Font<'a> {
    /// # Errors
    /// Возвращает ошибку неподдерживаемого формата или недопустимых размеров атласа.
    pub fn parse(source: &'a str) -> Result<Self> {
        let records: Vec<_> = source
            .split_inclusive('\n')
            .map(|line| {
                let doc = Document::parse(line)?;
                Ok(Record {
                    kind: doc.nodes.first().and_then(Node::atom).unwrap_or(""),
                    doc,
                })
            })
            .collect::<Result<_>>()?;
        let info = records
            .iter()
            .find(|record| record.kind == "info")
            .context("Missing bitmap font info")?;
        let common = records
            .iter()
            .find(|record| record.kind == "common")
            .context("Missing bitmap font common metrics")?;
        let size = atom_number(field(&info.doc, "size")?)?.abs();
        let width = unsigned(&common.doc, "scaleW")?;
        let height = unsigned(&common.doc, "scaleH")?;
        ensure!(
            size > 0.0
                && width > 0
                && height > 0
                && u64::from(width) * u64::from(height) <= 64_000_000,
            "Invalid bitmap font dimensions"
        );
        ensure!(
            unsigned(&common.doc, "pages")? == 1,
            "Only single-page bitmap fonts are supported"
        );
        Ok(Self {
            records,
            size,
            dimensions: (width, height),
        })
    }

    /// # Errors
    /// Возвращает ошибку метрик, размеров результата или пересекающихся правок.
    pub fn resize(&self, target_size: f64, page_name: &str) -> Result<(String, (u32, u32))> {
        let factor = target_size / self.size;
        let dimension = |value| (f64::from(value) * factor).round_ties_even().max(1.0);
        let width = dimension(self.dimensions.0);
        let height = dimension(self.dimensions.1);
        ensure!(
            target_size.is_finite() && target_size >= 1.0 && width * height <= 64_000_000.0,
            "Scaled bitmap font exceeds processing limits"
        );
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "Размеры положительны и ограничены лимитом изображения"
        )]
        let dimensions = (width as u32, height as u32);
        let mut output = String::new();
        for record in &self.records {
            let mut edits = Edits::default();
            for node in &record.doc.nodes {
                match node.key {
                    Some("scaleW") => edits.atom(node, dimensions.0.to_string()),
                    Some("scaleH") => edits.atom(node, dimensions.1.to_string()),
                    Some("file") if record.kind == "page" => {
                        edits.atom(node, format!("\"{page_name}\""));
                    }
                    Some("x" | "y" | "width" | "height") if record.kind == "char" => {}
                    Some(
                        "size" | "lineHeight" | "base" | "xoffset" | "yoffset" | "xadvance"
                        | "amount",
                    ) => {
                        let value = atom_number(node)?;
                        edits.atom(node, format!("{:.0}", (value * factor).round_ties_even()));
                    }
                    Some("padding" | "spacing") => {
                        let values = node
                            .atom()
                            .context("Invalid bitmap font spacing")?
                            .split(',')
                            .map(|value| {
                                let value = value.parse::<f64>()?;
                                ensure!(value.is_finite(), "Invalid bitmap font spacing");
                                Ok(format!("{:.0}", (value * factor).round_ties_even()))
                            })
                            .collect::<Result<Vec<_>>>()?;
                        edits.atom(node, values.join(","));
                    }
                    _ => {}
                }
            }
            if record.kind == "char" {
                glyph_rect(&record.doc, self.dimensions, dimensions, &mut edits)?;
            }
            output.push_str(&edits.apply(record.doc.source)?);
        }
        Ok((output, dimensions))
    }
}

fn field<'doc, 'source>(doc: &'doc Document<'source>, key: &str) -> Result<&'doc Node<'source>> {
    doc.nodes
        .iter()
        .find(|node| node.key == Some(key))
        .with_context(|| format!("Missing bitmap font {key}"))
}

fn unsigned(doc: &Document<'_>, key: &str) -> Result<u32> {
    field(doc, key)?
        .atom()
        .with_context(|| format!("Missing bitmap font {key}"))?
        .parse()
        .with_context(|| format!("Invalid bitmap font {key}"))
}

fn atom_number(node: &Node<'_>) -> Result<f64> {
    let value = node
        .atom()
        .context("Invalid bitmap font metric")?
        .parse::<f64>()?;
    ensure!(value.is_finite(), "Non-finite bitmap font metric");
    Ok(value)
}

fn glyph_rect(
    doc: &Document<'_>,
    original: (u32, u32),
    target: (u32, u32),
    edits: &mut Edits,
) -> Result<()> {
    for (position, extent, source, destination) in [
        ("x", "width", original.0, target.0),
        ("y", "height", original.1, target.1),
    ] {
        let position = field(doc, position)?;
        let extent = field(doc, extent)?;
        let start = atom_number(position)?;
        let length = atom_number(extent)?;
        ensure!(
            start >= 0.0 && length >= 0.0 && start + length <= f64::from(source),
            "Bitmap glyph is outside its atlas"
        );
        let map =
            |value: f64| (value * f64::from(destination) / f64::from(source)).round_ties_even();
        for (node, value) in [
            (position, map(start)),
            (extent, map(start + length) - map(start)),
        ] {
            edits.atom(node, format!("{value:.0}"));
        }
    }
    Ok(())
}

/// # Errors
/// Возвращает ошибку чтения шрифтов, масштабирования атласов или записи результата.
pub fn compile(
    source: &str,
    game: &Installation,
    profile: &Profile,
    write: &mut dyn FnMut(&str, &[u8]) -> Result<()>,
) -> Result<String> {
    let doc = Document::parse(source)?;
    let catalog = Catalog::from_document(&doc);
    let mut edits = Edits::default();
    compile_edits(&doc, game, profile, &catalog, write, &mut edits)?;
    edits.apply(source)
}

pub(super) fn compile_edits(
    doc: &Document<'_>,
    game: &Installation,
    profile: &Profile,
    catalog: &Catalog,
    write: &mut dyn FnMut(&str, &[u8]) -> Result<()>,
    edits: &mut Edits,
) -> Result<()> {
    let mut generated = BTreeMap::new();
    doc.try_walk(|node| -> Result<()> {
        if !matches!(node.key, Some("bitmapfont" | "bitmapfont_override"))
            || node.field("ttf_font").is_some()
        {
            return Ok(());
        }
        let Some(name) = node.named() else {
            return Ok(());
        };
        let Some(path) = bitmap_paths(node).find_map(Node::atom) else {
            return Ok(());
        };
        let default = crate::config::FontRule::default();
        let rule = profile.fonts.get(name).unwrap_or(&default);
        let Some(group) = catalog.groups.iter().find(|group| group.name == name) else {
            return Ok(());
        };
        if !group.uses_bitmap(path, rule, &catalog.languages, profile.font_mode) {
            return Ok(());
        }
        for reference in bitmap_paths(node) {
            let Some(path) = reference.atom() else {
                continue;
            };
            let key = (name, path);
            let target = match generated.entry(key) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(generate(game, profile, name, path, rule, write)?)
                }
            };
            edits.atom(reference, format!("\"{target}\""));
        }
        Ok(())
    })?;
    Ok(())
}

fn generate(
    game: &Installation,
    profile: &Profile,
    name: &str,
    path: &str,
    rule: &crate::config::FontRule,
    write: &mut dyn FnMut(&str, &[u8]) -> Result<()>,
) -> Result<String> {
    let source = game.text(&format!("{path}.fnt"))?;
    let font = Font::parse(&source).with_context(|| format!("Cannot parse bitmap font: {path}"))?;
    let base = rule.base_size.map_or(font.size, f64::from);
    let size = scaled_size(
        base,
        rule.scale.or(profile.font_scale).unwrap_or(profile.scale),
    );
    let stem = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .context("Invalid bitmap font path")?;
    let relative = path.strip_prefix("gfx/fonts/").unwrap_or(path);
    let target = format!("gfx/fonts/ui_scaler/{name}/{relative}");
    let (metrics, dimensions) = font.resize(size, &format!("{stem}.dds"))?;
    let atlas = ["dds", "tga", "png"]
        .into_iter()
        .map(|extension| format!("{path}.{extension}"))
        .find(|path| game.contains(path))
        .with_context(|| format!("Missing bitmap font atlas: {path}"))?;
    let image = textures::decode(&game.read(&atlas)?)?;
    ensure!(
        image.dimensions() == font.dimensions,
        "Bitmap atlas dimensions do not match metrics: {path}"
    );
    let image = textures::resize(
        &image,
        dimensions.0,
        dimensions.1,
        &profile.options(&atlas, crate::config::Category::Artwork),
    )?;
    write(&format!("{target}.fnt"), metrics.as_bytes())?;
    write(&format!("{target}.dds"), &textures::resource_dds(&image)?)?;
    Ok(target)
}
