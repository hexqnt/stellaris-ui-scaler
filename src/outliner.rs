// Размеры ограничены профилем и лимитом изображения; преобразования каналов насыщаются явно.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
mod layout;

use anyhow::Result;

use crate::{
    assets::Diagnostic,
    config::{OUTLINER_REFERENCE_SCALE, OutlinerOptions, Scale},
    pdx::{Document, Edits, Node},
};

pub const ROW_BACKGROUND: &str = "gfx/interface/buttons/outliner_entry_compact_scaled.png";
pub const SELECTED_BACKGROUND: &str =
    "gfx/interface/buttons/outliner_selected_entry_compact_scaled.png";
pub const SHARED_BACKGROUND: &str = "GFX_ui_scaler_member_bg";

/// # Errors
/// Возвращает ошибку синтаксиса или конфликтующих правок общих элементов компактной панели.
pub fn shared_gui(
    source: &str,
    path: &str,
    scale: Scale,
    options: &OutlinerOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<String> {
    if !options.compact {
        return Ok(source.into());
    }
    let base = path.strip_prefix("interface/").unwrap_or(path);
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    if base == "alerts.gui" {
        let mut matches = Vec::new();
        doc.walk(|node| {
            if node.named() == Some("toast_container") {
                matches.push(node);
            }
        });
        if let [node] = matches.as_slice() {
            let x = -(f64::from(options.width) + 450.0) * scale.get() / OUTLINER_REFERENCE_SCALE;
            for field in ["show_position", "hide_position"] {
                if let Some(property) = node.field(field).and_then(|node| node.field("x")) {
                    edits.atom(property, format!("{:.0}", x.round_ties_even()));
                } else {
                    diagnostics.push(Diagnostic {
                        path: path.into(),
                        span: Some(node.span.clone()),
                        rule: "compact-toast-position".into(),
                        message: format!("Missing toast_container.{field}.x"),
                    });
                }
            }
        } else {
            diagnostics.push(Diagnostic {
                path: path.into(),
                span: None,
                rule: "compact-toast-position".into(),
                message: format!("Expected one toast_container, found {}", matches.len()),
            });
        }
    }
    if ["main.gui", "combat_view.gui", "waystation_network_view.gui"].contains(&base) {
        doc.walk(|node| {
            if node.atom() == Some("GFX_outliner_member_bg") {
                edits.atom(node, format!("\"{SHARED_BACKGROUND}\""));
            }
        });
    }
    edits.apply(source)
}

#[must_use]
pub fn dimensions(options: &OutlinerOptions, scale: Scale) -> (u32, u32, u32) {
    let ratio = scale.get() / OUTLINER_REFERENCE_SCALE;
    let convert = |v: u32| (f64::from(v) * ratio).round_ties_even() as u32;
    (
        convert(options.width),
        convert(options.row_height),
        convert(options.title_height),
    )
}

fn set(node: &Node<'_>, field: &str, value: String, edits: &mut Edits) {
    if let Some(property) = node.field(field) {
        edits.atom(property, value);
    } else {
        edits.insert(node, &format!("\n\t\t{field} = {value}\n\t"));
    }
}
fn xy(node: &Node<'_>, field: &str, x: i64, y: i64, edits: &mut Edits) {
    if let Some(block) = node.field(field) {
        for (names, value) in [(["x", "width"], x), (["y", "height"], y)] {
            if let Some(property) = names.iter().find_map(|key| block.field(key)) {
                edits.atom(property, value.to_string());
            }
        }
    } else {
        edits.insert(node, &format!("\n\t\t{field} = {{ x = {x} y = {y} }}\n\t"));
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "Последовательность преобразований сохраняет порядок зависимых этапов"
)]
/// # Errors
/// Возвращает ошибку синтаксиса или конфликтующих правок компоновки.
pub fn transform(source: &str, options: &OutlinerOptions, scale: Scale) -> Result<String> {
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    let ratio = scale.get() / OUTLINER_REFERENCE_SCALE;
    let p = |v: f64| (v * ratio).round_ties_even() as i64;
    let (width, height, title) = dimensions(options, scale);
    let width = i64::from(width);
    let mut rows = Vec::new();
    doc.walk(|node| {
        let Some(name) = node.named() else {
            return;
        };
        if name.starts_with("outliner_")
            && name.ends_with("entry_window")
            && (node.field("background").is_some()
                || name.contains("title")
                || node.children().iter().any(|c| {
                    c.field("quadTextureSprite").and_then(Node::atom)
                        == Some("GFX_outliner_member_bg")
                }))
        {
            rows.push(node);
        }
        if name == "outliner_tooltip_offset"
            && let Some(x) = node.field("position").and_then(|n| n.field("x"))
        {
            edits.atom(x, (-width / 2).to_string());
        }
    });
    // Ширина окна и списка меняется только в собственных полях контейнеров.
    doc.walk(|node| {
        if let Some(size) = node.field("size")
            && let Some(w) = size.field("width").or_else(|| size.field("x"))
            && let Some(v) = w.parse_atom::<i64>()
        {
            if v == scale.pixels(260.0) as i64 {
                edits.atom(w, width.to_string());
            } else if v == scale.pixels(240.0) as i64 {
                edits.atom(w, (width - p(28.0)).to_string());
            }
        }
    });
    for row in rows {
        let name = row.named().unwrap_or_default();
        let section = name == "outliner_title_entry_window";
        let row_h = i64::from(if section { title } else { height });
        // Размеры полных строк обычно отличаются от ширины самого окна.
        let already_panel = row
            .field("size")
            .and_then(|s| s.field("width"))
            .and_then(Node::parse_atom::<i64>)
            == Some(scale.pixels(260.0) as i64);
        if already_panel {
            if let Some(h) = row.field("size").and_then(|s| s.field("height")) {
                edits.atom(h, row_h.to_string());
            }
        } else {
            xy(row, "size", width, row_h, &mut edits);
        }
        let mut texts: Vec<_> = row
            .children()
            .iter()
            .filter(|n| {
                n.key == Some("instantTextBoxType")
                    && ![
                        Some("name"),
                        Some("title"),
                        Some("subject_name"),
                        Some("situation_name"),
                        Some("header"),
                    ]
                    .contains(&n.named())
            })
            .collect();
        texts.sort_by_key(|n| {
            n.field("position")
                .and_then(|s| s.field("y"))
                .and_then(Node::parse_atom::<i64>)
                .is_none_or(|y| y < p(20.0))
        });
        for child in row.children() {
            let child_name = child.named().unwrap_or_default();
            if child_name == "background" {
                xy(child, "position", 0, 0, &mut edits);
                xy(child, "size", width, row_h, &mut edits);
            }
            if child_name == "selected_overlay" {
                xy(child, "size", width, row_h, &mut edits);
            }
            if child_name == "list" && name.contains("title") {
                xy(child, "position", 0, row_h - p(2.0), &mut edits);
                if let Some(w) = child.field("size").and_then(|s| s.field("x")) {
                    edits.atom(w, width.to_string());
                }
            }
            if child.key == Some("instantTextBoxType") {
                let primary = ["name", "title", "subject_name", "situation_name", "header"]
                    .contains(&child_name);
                let slot = texts.iter().position(|n| n.span == child.span).unwrap_or(2);
                let mut x = if primary {
                    p(40.0)
                } else if slot == 0 {
                    width - p(320.0)
                } else {
                    width - p(85.0)
                };
                let mut maxw = if primary {
                    p(230.0)
                } else if slot == 0 {
                    p(225.0)
                } else {
                    p(65.0)
                };
                let mut align = if primary || slot == 0 {
                    "left"
                } else {
                    "right"
                };
                let mut y = p(2.0);
                if section && primary {
                    x = 0;
                    maxw = width;
                    align = "center";
                }
                if name.contains("planet_entry") || name.contains("arkship_colony") {
                    if primary {
                        maxw = p(150.0);
                    }
                    if child_name == "colony_type" {
                        x = p(200.0);
                        maxw = p(150.0);
                    }
                }
                if name.contains("starbase_entry") && primary {
                    maxw = p(205.0);
                }
                if name.contains("starbase_entry") && child_name == "military_power" {
                    x = width - p(512.0);
                    maxw = p(225.0);
                    align = "right";
                }
                if name.contains("fleet_entry") {
                    if child_name == "offensive_power" {
                        x = width - p(115.0);
                        maxw = p(95.0);
                        align = "right";
                    }
                    if child_name == "size_limit" {
                        x = width - p(345.0);
                        maxw = p(225.0);
                        align = "right";
                    }
                }
                if name.contains("army_entry") && child_name == "army_power" {
                    x = p(95.0);
                    maxw = p(225.0);
                    align = "right";
                }
                if name.contains("megastructure") || name.contains("terraforming") {
                    if primary {
                        maxw = p(205.0);
                    }
                    if ["system", "star"].contains(&child_name) {
                        x = width - p(85.0);
                        maxw = p(65.0);
                    }
                }
                if !primary && slot >= 2 {
                    x = width + p(2000.0);
                    y = 0;
                    maxw = 1;
                }
                xy(child, "position", x, y, &mut edits);
                set(child, "maxWidth", maxw.to_string(), &mut edits);
                set(
                    child,
                    "maxHeight",
                    if maxw == 1 { 1 } else { row_h - p(4.0) }.to_string(),
                    &mut edits,
                );
                set(child, "format", align.into(), &mut edits);
                set(child, "Orientation", "\"UPPER_LEFT\"".into(), &mut edits);
                set(child, "vertical_alignment", "center".into(), &mut edits);
                if !section {
                    set(child, "font", "\"cg_16b\"".into(), &mut edits);
                }
            }
            if child.key == Some("iconType") && child_name != "background" {
                let sprite = child
                    .field("spriteType")
                    .and_then(Node::atom)
                    .unwrap_or_default();
                if sprite.contains("progressbar") {
                    xy(
                        child,
                        "position",
                        width - p(320.0),
                        (row_h - p(11.0)) / 2,
                        &mut edits,
                    );
                } else {
                    let x = child
                        .field("position")
                        .and_then(|s| s.field("x"))
                        .and_then(Node::parse_atom::<i64>)
                        .unwrap_or(0);
                    let centered =
                        child.field("centerPosition").and_then(Node::atom) == Some("yes");
                    xy(
                        child,
                        "position",
                        x,
                        if centered { row_h / 2 } else { p(3.0) },
                        &mut edits,
                    );
                    let old_scale = child
                        .field("scale")
                        .and_then(Node::parse_atom::<f64>)
                        .unwrap_or(1.0);
                    set(
                        child,
                        "scale",
                        format!("{:.4}", old_scale * 0.534),
                        &mut edits,
                    );
                }
            }
        }
    }
    let source = edits.apply(source)?;
    layout::apply_special(&source, width, ratio)
}
