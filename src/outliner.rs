// Размеры ограничены профилем и лимитом изображения; преобразования каналов насыщаются явно.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
mod layout;
mod rows;

use std::fmt::Display;

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
    let base = path.trim_prefix("interface/");
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    if base == "alerts.gui" {
        let mut found = None;
        let mut count = 0;
        doc.walk(|node| {
            if node.named() == Some("toast_container") {
                found = Some(node);
                count += 1;
            }
        });
        if let Some(node) = found.filter(|_| count == 1) {
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
                message: format!("Expected one toast_container, found {count}"),
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

fn set(node: &Node<'_>, field: &str, value: impl Display, edits: &mut Edits) {
    if let Some(property) = node.field(field) {
        edits.atom(property, value.to_string());
    } else {
        edits.insert_text(node, format!("\n\t\t{field} = {value}\n\t"));
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
        edits.insert_text(node, format!("\n\t\t{field} = {{ x = {x} y = {y} }}\n\t"));
    }
}

/// # Errors
/// Возвращает ошибку синтаксиса или конфликтующих правок компоновки.
pub fn transform(source: &str, options: &OutlinerOptions, scale: Scale) -> Result<String> {
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    let ratio = scale.get() / OUTLINER_REFERENCE_SCALE;
    let p = |v: f64| (v * ratio).round_ties_even() as i64;
    let (width, height, title) = dimensions(options, scale);
    let width = i64::from(width);
    let panel_width = scale.pixels(260.0) as i64;
    let list_width = scale.pixels(240.0) as i64;
    let mut rows = Vec::new();
    doc.walk(|node| {
        if let Some(name) = node.named() {
            if rows::is_entry(name)
                && (node.field("background").is_some()
                    || name.contains("title")
                    || node.children().iter().any(|child| {
                        child.field("quadTextureSprite").and_then(Node::atom)
                            == Some("GFX_outliner_member_bg")
                    }))
            {
                rows.push((node, name));
            }
            if name == "outliner_tooltip_offset"
                && let Some(x) = node.field("position").and_then(|n| n.field("x"))
            {
                edits.atom(x, (-width / 2).to_string());
            }
        }
        // Ширина окна и списка меняется только в собственных полях контейнеров.
        if let Some(size) = node.field("size")
            && let Some(w) = size.field("width").or_else(|| size.field("x"))
            && let Some(v) = w.parse_atom::<i64>()
        {
            if v == panel_width {
                edits.atom(w, width.to_string());
            } else if v == list_width {
                edits.atom(w, (width - p(28.0)).to_string());
            }
        }
    });
    for (row, name) in rows {
        let height = if name == rows::TITLE { title } else { height };
        rows::apply(
            row,
            name,
            width,
            i64::from(height),
            panel_width,
            p,
            &mut edits,
        );
    }
    layout::apply_special(&edits.apply(source)?, width, ratio)
}
