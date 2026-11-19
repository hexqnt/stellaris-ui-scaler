mod areas;

use std::{collections::BTreeMap, sync::Arc};

use crate::pdx::{Document, Node};

use super::{Catalog, Group};

pub use areas::UsageArea;

/// Явное назначение группы шрифтов в эффективном GUI игры и выбранных модов.
#[derive(Debug, Clone)]
pub struct Usage {
    pub area: Option<UsageArea>,
    pub file: Arc<str>,
    pub line: usize,
    pub widget: Option<String>,
    pub kind: String,
    pub property: String,
}

#[derive(Default)]
pub struct Index(BTreeMap<String, Vec<Usage>>);

impl Index {
    pub(crate) fn read_gui(&mut self, path: &str, doc: &Document<'_>) {
        let file: Arc<str> = path.into();
        let mut cursor = 0;
        let mut line = 1;
        let mut visit = |block: &Node<'_>, field: &Node<'_>, area| {
            let Some(property) = field.key.filter(|key| {
                key.eq_ignore_ascii_case("font") || key.eq_ignore_ascii_case("buttonFont")
            }) else {
                return;
            };
            let Some(font) = field.atom() else {
                return;
            };
            line += doc.source[cursor..field.span.start].matches('\n').count();
            cursor = field.span.start;
            self.0.entry(font.into()).or_default().push(Usage {
                area,
                file: Arc::clone(&file),
                line,
                widget: block.named().map(str::to_owned),
                kind: block.key.unwrap_or("block").into(),
                property: property.into(),
            });
        };
        for node in &doc.nodes {
            walk_fields(path, node, UsageArea::in_file(path), &mut visit);
        }
    }

    pub(crate) fn apply(mut self, catalog: &mut Catalog) {
        for group in &mut catalog.groups {
            group.usages = self.0.remove(&group.name).unwrap_or_default();
            group
                .usages
                .sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        }
    }
}

impl Group {
    #[must_use]
    pub fn uses_area(&self, area: UsageArea) -> bool {
        self.usages.iter().any(|usage| usage.area == Some(area))
            || UsageArea::in_group(&self.name) == Some(area)
    }

    pub fn usage_areas(&self) -> impl Iterator<Item = UsageArea> + '_ {
        UsageArea::ALL
            .iter()
            .copied()
            .filter(|&area| self.uses_area(area))
    }

    /// Типичная роль штатной группы; фактические назначения из модов перечислены отдельно.
    #[must_use]
    pub fn usage_note(&self) -> Option<&'static str> {
        Some(match self.name.as_str() {
            "cg_16b" => {
                "Common interface text: descriptions, buttons, tooltips and numerical values."
            }
            "malgun_goth_24" => {
                "Window and section headings; planet, fleet, ship and station names."
            }
            "hoi_16mbs" => {
                "Map resource amounts and ship counts; also chat and some station controls."
            }
            "jura" => {
                "Start, load, multiplayer, settings, credits and quit labels in the main menu."
            }
            "large_title_font" => {
                "Prominent headings and status messages, including pause and saving."
            }
            "large_title_font_28" => {
                "System and empire names in the bottom map bar; also some DLC headings and prices."
            }
            "load_screen" => "Loading progress text and loading screen tips.",
            "map_name_border" => {
                "Likely empire names inside map borders, inferred from the built-in group name."
            }
            "map_name_nebula" => {
                "Likely nebula names on the map, inferred from the built-in group name."
            }
            "map_name_sector" => {
                "Likely sector names on the map, inferred from the built-in group name."
            }
            "standard_font" | "null_font" => {
                "Built-in font group without a confirmed interface role. Its name alone does not establish where the engine uses it."
            }
            _ => return None,
        })
    }
}

fn walk_fields(
    path: &str,
    node: &Node<'_>,
    inherited: Option<UsageArea>,
    visit: &mut impl FnMut(&Node<'_>, &Node<'_>, Option<UsageArea>),
) {
    let area = UsageArea::in_block(path, node, inherited);
    for field in node.children() {
        if field.atom().is_some() {
            visit(node, field, area);
        } else {
            walk_fields(path, field, area, visit);
        }
    }
}

#[cfg(test)]
mod tests;
