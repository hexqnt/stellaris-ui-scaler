pub mod bitmap;
mod catalog;

use std::collections::BTreeMap;

use anyhow::Result;

use crate::{
    assets::Installation,
    config::{FontMode, FontRule, Scale},
    pdx::{Document, Edits, Node, Value},
};

pub use catalog::{BitmapVariant, Catalog, Group, Resolved, Variant};

const EXTENDED: &[&str] = &[
    "l_english",
    "l_braz_por",
    "l_german",
    "l_french",
    "l_spanish",
    "l_polish",
    "l_russian",
];

const LATIN: &[&str] = EXTENDED.split_at(5).0;

#[derive(Clone, Copy)]
pub struct Replacement {
    pub family: &'static str,
    pub base_size: f64,
    pub languages: &'static [&'static str],
}

#[must_use]
pub fn replacement(name: &str) -> Option<Replacement> {
    let (family, base_size, languages) = match name {
        "cg_16b" => ("Easter_normal", 13.0, LATIN),
        "malgun_goth_24" => ("Easter_header", 18.0, LATIN),
        "large_title_font_28" => ("Easter_header", 26.0, LATIN),
        "large_title_font" => ("Easter_bigger", 30.0, LATIN),
        "jura" => ("Jura_ttf", 36.0, EXTENDED),
        "hoi_16mbs" => ("Easter_normal", 16.0, EXTENDED),
        _ => return None,
    };
    Some(Replacement {
        family,
        base_size,
        languages,
    })
}

#[must_use]
pub fn is_font_file(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| name.contains("font"))
}

#[must_use]
pub fn scaled_size(size: f64, scale: Scale) -> f64 {
    (size * scale.get()).floor().max(1.0)
}

/// # Errors
/// Возвращает ошибку синтаксиса или пересекающихся правок.
pub fn transform(source: &str, scale: Scale) -> Result<String> {
    let doc = Document::parse(source)?;
    let catalog = Catalog::from_document(&doc);
    let mut edits = Edits::default();
    transform_edits(
        &doc,
        scale,
        &BTreeMap::new(),
        &catalog,
        FontMode::Automatic,
        &mut edits,
    );
    edits.apply(source)
}

/// # Errors
/// Возвращает ошибку синтаксиса или пересекающихся правок настроенных шрифтов.
pub fn transform_configured(
    source: &str,
    scale: Scale,
    rules: &BTreeMap<String, FontRule>,
    catalog: &Catalog,
    mode: FontMode,
) -> Result<String> {
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    transform_edits(&doc, scale, rules, catalog, mode, &mut edits);
    edits.apply(source)
}

pub(crate) fn compile(
    source: &str,
    game: &Installation,
    profile: &crate::config::Profile,
    catalog: &Catalog,
    write: &mut dyn FnMut(&str, &[u8]) -> Result<()>,
) -> Result<String> {
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    bitmap::compile_edits(&doc, game, profile, catalog, write, &mut edits)?;
    transform_edits(
        &doc,
        profile.font_scale.unwrap_or(profile.scale),
        &profile.fonts,
        catalog,
        profile.font_mode,
        &mut edits,
    );
    edits.apply(source)
}

fn transform_edits(
    doc: &Document<'_>,
    scale: Scale,
    rules: &BTreeMap<String, FontRule>,
    catalog: &Catalog,
    mode: FontMode,
    edits: &mut Edits,
) {
    doc.walk(|node| {
        let rule = node.named().and_then(|name| rules.get(name));
        if let Some(size_node) = node.field("ttf_size")
            && let Value::Atom { text, .. } = &size_node.value
            && let Some(size) = positive_size(size_node)
        {
            let base = rule.and_then(|rule| rule.base_size).map_or(size, f64::from);
            let size = scaled_size(base, rule.and_then(|rule| rule.scale).unwrap_or(scale));
            edits.atom(
                size_node,
                if text.starts_with('"') {
                    format!("\"{size:.0}\"")
                } else {
                    format!("{size:.0}")
                },
            );
            if let Some(family) = rule.and_then(|rule| rule.family.as_deref())
                && let Some(font) = node.field("ttf_font")
            {
                edits.atom(font, format!("\"{family}\""));
            }
        }
    });
    for root in doc
        .nodes
        .iter()
        .filter(|node| node.key == Some("bitmapfonts"))
    {
        for node in root
            .children()
            .iter()
            .filter(|node| node.key == Some("bitmapfont"))
        {
            let Some(name) = node.named() else {
                continue;
            };
            let Some(group) = catalog.groups.iter().find(|group| group.name == name) else {
                continue;
            };
            let default = FontRule::default();
            let rule = rules.get(name).unwrap_or(&default);
            let mut by_size = BTreeMap::<String, Vec<String>>::new();
            for language in &catalog.languages {
                if group.native(language).is_none()
                    && group.replacement_enabled(language, rule, mode)
                {
                    let size = scaled_size(
                        group.replacement_base(language, rule),
                        rule.scale.unwrap_or(scale),
                    );
                    by_size
                        .entry(format!("{size:.0}"))
                        .or_default()
                        .push(format!("\"{language}\""));
                }
            }
            let family = group.replacement_family(rule);
            for (size, languages) in by_size {
                edits.insert(root, &format!("\n\tbitmapfont_override = {{ name = \"{name}\" ttf_font = \"{family}\" ttf_size = \"{size}\" languages = {{ {} }} }}\n", languages.join(" ")));
            }
        }
    }
}

/// # Errors
/// Возвращает ошибку синтаксиса исходного файла.
pub fn definitions(source: &str) -> Result<String> {
    let doc = Document::parse(source)?;
    if doc
        .nodes
        .iter()
        .any(|node| node.named() == Some("Jura_ttf"))
    {
        return Ok(source.into());
    }
    Ok(format!(
        "{source}\nfont = {{ name = \"Jura_ttf\" fontstyle = {{ style = regular file = \"gfx/fonts/Jura-VariableFont_wght.ttf\" }} }}\n"
    ))
}

fn positive_size(node: &Node<'_>) -> Option<f64> {
    node.parse_atom::<f64>()
        .filter(|size| size.is_finite() && *size > 0.0)
}

pub(super) fn bitmap_paths<'node, 'source>(
    node: &'node Node<'source>,
) -> impl Iterator<Item = &'node Node<'source>> {
    node.field("path")
        .into_iter()
        .chain(node.field("fontfiles").into_iter().flat_map(Node::children))
}
