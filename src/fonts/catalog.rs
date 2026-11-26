use std::collections::BTreeMap;

use anyhow::{Result, ensure};

use crate::{
    assets::Installation,
    config::{FontMode, FontRule, Scale},
    pdx::{Document, Node},
};

use super::{EXTENDED, bitmap_paths, is_font_file, positive_size, replacement, scaled_size};

#[derive(Debug, Clone)]
pub struct Variant {
    pub family: String,
    pub size: f64,
    pub languages: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BitmapVariant {
    pub path: String,
    pub face: Option<String>,
    pub size: Option<f64>,
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Group {
    pub name: String,
    pub usages: Vec<super::Usage>,
    pub bitmap_path: Option<String>,
    pub bitmap: bool,
    pub bitmap_face: Option<String>,
    pub bitmap_size: Option<f64>,
    pub variants: Vec<Variant>,
    pub bitmap_variants: Vec<BitmapVariant>,
}

impl Group {
    /// Исходный шрифт выбранного языка до применения пользовательских настроек.
    #[must_use]
    pub fn original(&self, language: &str) -> Resolved<'_> {
        let (family, size, ttf) = self.native(language).map_or_else(
            || {
                let (family, size) = self.bitmap_info(language);
                (family, size, false)
            },
            |native| (native.family.as_str(), Some(native.size), true),
        );
        Resolved {
            family,
            base_size: size,
            size,
            ttf,
        }
    }

    #[must_use]
    pub fn native(&self, language: &str) -> Option<&Variant> {
        last_variant(&self.variants, language, |variant| &variant.languages)
    }

    #[must_use]
    pub fn bitmap_info(&self, language: &str) -> (&str, Option<f64>) {
        self.bitmap_variant(language).map_or_else(
            || {
                (
                    self.bitmap_face.as_deref().unwrap_or("Bitmap"),
                    self.bitmap_size,
                )
            },
            |variant| (variant.face.as_deref().unwrap_or("Bitmap"), variant.size),
        )
    }

    fn bitmap_variant(&self, language: &str) -> Option<&BitmapVariant> {
        last_variant(&self.bitmap_variants, language, |variant| {
            &variant.languages
        })
    }

    /// Проверяет, остаётся ли этот растровый вариант активным хотя бы для одного языка.
    pub(super) fn uses_bitmap(
        &self,
        path: &str,
        rule: &FontRule,
        languages: &[String],
        mode: FontMode,
    ) -> bool {
        languages.iter().any(|language| {
            self.native(language).is_none()
                && !self.replacement_enabled(language, rule, mode)
                && self
                    .bitmap_variant(language)
                    .map(|variant| variant.path.as_str())
                    .or(self.bitmap_path.as_deref())
                    == Some(path)
        })
    }

    #[must_use]
    pub fn replacement_enabled(&self, language: &str, rule: &FontRule, mode: FontMode) -> bool {
        self.bitmap
            && rule.replace_bitmap.unwrap_or_else(|| {
                replacement(&self.name).is_some_and(|spec| match mode {
                    FontMode::Automatic => spec.languages.contains(&language),
                    FontMode::TrueType => EXTENDED.contains(&language),
                    FontMode::Original => false,
                })
            })
    }

    #[must_use]
    pub fn replacement_family<'a>(&'a self, rule: &'a FontRule) -> &'a str {
        rule.family
            .as_deref()
            .unwrap_or_else(|| replacement(&self.name).map_or("Easter_normal", |spec| spec.family))
    }

    #[must_use]
    pub fn replacement_base(&self, language: &str, rule: &FontRule) -> f64 {
        rule.base_size.map_or_else(
            || {
                replacement(&self.name).map_or_else(
                    || self.bitmap_info(language).1.unwrap_or(16.0),
                    |spec| spec.base_size,
                )
            },
            f64::from,
        )
    }

    #[must_use]
    pub fn resolve<'a>(
        &'a self,
        language: &str,
        rule: &'a FontRule,
        scale: Scale,
        mode: FontMode,
    ) -> Resolved<'a> {
        let mut resolved = self.original(language);
        if !resolved.ttf && self.replacement_enabled(language, rule, mode) {
            resolved.family = self.replacement_family(rule);
            resolved.base_size = Some(self.replacement_base(language, rule));
            resolved.ttf = true;
        } else {
            resolved.base_size = rule.base_size.map(f64::from).or(resolved.base_size);
            if resolved.ttf {
                resolved.family = rule.family.as_deref().unwrap_or(resolved.family);
            }
        }
        resolved.size = resolved
            .base_size
            .map(|size| scaled_size(size, rule.scale.unwrap_or(scale)));
        resolved
    }
}

fn last_variant<'a, T>(
    variants: &'a [T],
    language: &str,
    languages: impl Fn(&T) -> &[String],
) -> Option<&'a T> {
    variants.iter().rev().find(|variant| {
        let supported = languages(variant);
        supported.is_empty() || supported.iter().any(|item| item == language)
    })
}

pub struct Resolved<'a> {
    pub family: &'a str,
    pub base_size: Option<f64>,
    pub size: Option<f64>,
    pub ttf: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub groups: Vec<Group>,
    pub families: Vec<String>,
    pub languages: Vec<String>,
}

impl Catalog {
    pub(super) fn from_document(doc: &Document<'_>) -> Self {
        let mut catalog = Self::default();
        catalog.read_document(doc);
        catalog.finish();
        catalog
    }

    /// # Errors
    /// Возвращает ошибку синтаксиса объявлений или чтения метрик исходного шрифта.
    pub fn load(game: &Installation, sources: &BTreeMap<String, String>) -> Result<Self> {
        let mut catalog = Self::default();
        for (path, source) in sources {
            if is_font_file(path) {
                catalog.read_document(&Document::parse(source)?);
            }
        }
        catalog.complete(game)?;
        Ok(catalog)
    }

    pub(crate) fn complete(&mut self, game: &Installation) -> Result<()> {
        for path in game.resources.keys().filter(|path| {
            path.starts_with("fonts/")
                && std::path::Path::new(path)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("asset"))
        }) {
            let source = game.text(path)?;
            let doc = Document::parse(&source)?;
            self.families
                .extend(doc.nodes.iter().filter_map(Node::named).map(str::to_owned));
        }
        if game.contains("gfx/fonts/Jura-VariableFont_wght.ttf") {
            self.families.push("Jura_ttf".into());
        }
        for group in &mut self.groups {
            if let Some(path) = &group.bitmap_path {
                (group.bitmap_face, group.bitmap_size) = bitmap_metadata(game, path)?;
            }
            for variant in &mut group.bitmap_variants {
                (variant.face, variant.size) = bitmap_metadata(game, &variant.path)?;
            }
        }
        self.finish();
        Ok(())
    }

    pub(crate) fn read_document(&mut self, doc: &Document<'_>) {
        doc.walk(|node| {
            if !matches!(node.key, Some("bitmapfont" | "bitmapfont_override")) {
                return;
            }
            let Some(name) = node.named() else {
                return;
            };
            let group = match self.groups.iter().position(|group| group.name == name) {
                Some(index) => &mut self.groups[index],
                None => self.groups.push_mut(Group {
                    name: name.into(),
                    ..Group::default()
                }),
            };
            let path = bitmap_paths(node).find_map(Node::atom);
            let languages: Vec<_> = node
                .field("languages")
                .map_or_default(Node::children)
                .iter()
                .filter_map(Node::atom)
                .map(str::to_owned)
                .collect();
            self.languages.extend(languages.iter().cloned());
            if node.key == Some("bitmapfont") {
                group.bitmap = true;
                group.bitmap_path = path.map(str::to_owned);
            } else if let Some(path) = path {
                group.bitmap_variants.push(BitmapVariant {
                    path: path.into(),
                    face: None,
                    size: None,
                    languages: languages.clone(),
                });
            }
            if let (Some(family), Some(size)) = (
                node.field("ttf_font").and_then(Node::atom),
                node.field("ttf_size").and_then(positive_size),
            ) {
                group.variants.push(Variant {
                    family: family.into(),
                    size,
                    languages,
                });
            }
        });
    }

    pub(super) fn finish(&mut self) {
        self.languages
            .extend(EXTENDED.iter().map(|language| (*language).into()));
        self.languages.sort();
        self.languages.dedup();
        self.groups
            .sort_by(|left, right| left.name.cmp(&right.name));
        self.families.sort();
        self.families.dedup();
    }

    /// # Errors
    /// Возвращает ошибку неизвестной группы или отсутствующего определения TTF.
    pub fn check_rules(&self, rules: &BTreeMap<String, FontRule>) -> Result<()> {
        for (name, rule) in rules {
            ensure!(
                self.groups.iter().any(|group| group.name == *name),
                "Unknown font group: {name}"
            );
            if let Some(family) = &rule.family {
                ensure!(
                    self.families.contains(family),
                    "Unknown TTF family for {name}: {family}"
                );
            }
        }
        Ok(())
    }
}

fn bitmap_metadata(game: &Installation, path: &str) -> Result<(Option<String>, Option<f64>)> {
    let metrics = format!("{path}.fnt");
    if !game.contains(&metrics) {
        return Ok((None, None));
    }
    let source = game.text(&metrics)?;
    let doc = Document::parse(&source)?;
    let face = doc
        .nodes
        .iter()
        .find(|node| node.key == Some("face"))
        .and_then(Node::atom)
        .map(str::to_owned);
    let size = doc
        .nodes
        .iter()
        .find(|node| node.key == Some("size"))
        .and_then(Node::parse_atom::<f64>)
        .filter(|size| size.is_finite())
        .map(f64::abs);
    Ok((face, size))
}
