use crate::pdx::{Edits, Node};

use super::{set, xy};

pub(super) const TITLE: &str = "outliner_title_entry_window";

pub(super) fn is_entry(name: &str) -> bool {
    name.starts_with("outliner_") && name.ends_with("entry_window")
}

fn primary(name: &str) -> bool {
    matches!(
        name,
        "name" | "title" | "subject_name" | "situation_name" | "header"
    )
}

pub(super) fn apply(
    row: &Node<'_>,
    name: &str,
    width: i64,
    height: i64,
    panel_width: i64,
    p: impl Fn(f64) -> i64,
    edits: &mut Edits,
) {
    // Размеры полных строк обычно отличаются от ширины самого окна.
    if let Some(size) = row.field("size")
        && size.field("width").and_then(Node::parse_atom::<i64>) == Some(panel_width)
    {
        if let Some(h) = size.field("height") {
            edits.atom(h, height.to_string());
        }
    } else {
        xy(row, "size", width, height, edits);
    }
    let texts = row.children().iter().filter(|node| {
        node.key == Some("instantTextBoxType") && !primary(node.named().unwrap_or_default())
    });
    let cutoff = p(20.0);
    let lower = |node: &&Node<'_>| {
        node.field("position")
            .and_then(|position| position.field("y"))
            .and_then(Node::parse_atom::<i64>)
            .is_some_and(|y| y >= cutoff)
    };
    // Нужны только первые два места. Сначала нижняя строка, внутри каждой группы сохраняем порядок.
    let mut ranked = texts
        .clone()
        .filter(lower)
        .chain(texts.filter(|node| !lower(node)));
    let slots = [ranked.next(), ranked.next()];
    for child in row.children() {
        let child_name = child.named().unwrap_or_default();
        match child_name {
            "background" => {
                xy(child, "position", 0, 0, edits);
                xy(child, "size", width, height, edits);
            }
            "selected_overlay" => xy(child, "size", width, height, edits),
            "list" if name.contains("title") => {
                xy(child, "position", 0, height - p(2.0), edits);
                if let Some(w) = child.field("size").and_then(|size| size.field("x")) {
                    edits.atom(w, width.to_string());
                }
            }
            _ => {}
        }
        match child.key {
            Some("instantTextBoxType") => {
                let slot = slots
                    .iter()
                    .position(|node| node.is_some_and(|node| node.span == child.span));
                text(child, name, child_name, slot, (width, height), &p, edits);
            }
            Some("iconType") if child_name != "background" => icon(child, width, height, &p, edits),
            _ => {}
        }
    }
}

fn text(
    child: &Node<'_>,
    row: &str,
    name: &str,
    slot: Option<usize>,
    (width, height): (i64, i64),
    p: impl Fn(f64) -> i64,
    edits: &mut Edits,
) {
    let primary = primary(name);
    let section = row == TITLE;
    let (mut x, mut max_width, mut align) = match (primary, section, slot) {
        (true, true, _) => (0, width, "center"),
        (true, false, _) => (p(40.0), p(230.0), "left"),
        (false, _, Some(0)) => (width - p(320.0), p(225.0), "left"),
        (false, _, _) => (width - p(85.0), p(65.0), "right"),
    };
    let planet = row.contains("planet_entry") || row.contains("arkship_colony");
    let starbase = row.contains("starbase_entry");
    if planet && primary {
        max_width = p(150.0);
    }
    if planet && name == "colony_type" {
        (x, max_width) = (p(200.0), p(150.0));
    }
    if starbase && primary {
        max_width = p(205.0);
    }
    let override_position = match name {
        "military_power" if starbase => Some((width - p(512.0), p(225.0))),
        "offensive_power" if row.contains("fleet_entry") => Some((width - p(115.0), p(95.0))),
        "size_limit" if row.contains("fleet_entry") => Some((width - p(345.0), p(225.0))),
        "army_power" if row.contains("army_entry") => Some((p(95.0), p(225.0))),
        _ => None,
    };
    if let Some((position, size)) = override_position {
        (x, max_width, align) = (position, size, "right");
    }
    if row.contains("megastructure") || row.contains("terraforming") {
        if primary {
            max_width = p(205.0);
        }
        if matches!(name, "system" | "star") {
            (x, max_width) = (width - p(85.0), p(65.0));
        }
    }
    let mut y = p(2.0);
    if !primary && slot.is_none() {
        (x, y, max_width) = (width + p(2000.0), 0, 1);
    }
    xy(child, "position", x, y, edits);
    set(child, "maxWidth", max_width, edits);
    set(
        child,
        "maxHeight",
        if max_width == 1 { 1 } else { height - p(4.0) },
        edits,
    );
    for (field, value) in [
        ("format", align),
        ("Orientation", "\"UPPER_LEFT\""),
        ("vertical_alignment", "center"),
    ] {
        set(child, field, value, edits);
    }
    if !section {
        set(child, "font", "\"cg_16b\"", edits);
    }
}

fn icon(child: &Node<'_>, width: i64, height: i64, p: impl Fn(f64) -> i64, edits: &mut Edits) {
    if child
        .field("spriteType")
        .and_then(Node::atom)
        .is_some_and(|name| name.contains("progressbar"))
    {
        xy(
            child,
            "position",
            width - p(320.0),
            (height - p(11.0)) / 2,
            edits,
        );
    } else {
        let x = child
            .field("position")
            .and_then(|s| s.field("x"))
            .and_then(Node::parse_atom::<i64>)
            .unwrap_or(0);
        let centered = child.field("centerPosition").and_then(Node::atom) == Some("yes");
        xy(
            child,
            "position",
            x,
            if centered { height / 2 } else { p(3.0) },
            edits,
        );
        let scale = child
            .field("scale")
            .and_then(Node::parse_atom::<f64>)
            .unwrap_or(1.0);
        set(child, "scale", format_args!("{:.4}", scale * 0.534), edits);
    }
}
