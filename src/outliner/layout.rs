#![allow(clippy::cast_possible_truncation)]
use anyhow::Result;

use crate::pdx::{Document, Edits, Node};

use super::{rows, set, xy};

#[derive(Default)]
struct Layout {
    position: Option<(i64, i64)>,
    size: Option<(i64, i64)>,
    max_width: Option<i64>,
    orientation: Option<&'static str>,
    format: Option<&'static str>,
}

impl Layout {
    const fn upper_right(&mut self, x: i64, y: i64) {
        self.position = Some((x, y));
        self.orientation = Some("UPPER_RIGHT");
    }

    const fn right_box(&mut self, x: i64, y: i64, width: i64, height: i64) {
        self.upper_right(x, y);
        self.size = Some((width, height));
        self.format = Some("right");
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "Таблица исключений хранит варианты строк вместе для сопоставления с журналом автора"
)]
fn special_layout(window: &str, name: &str, width: i64, ratio: f64) -> Layout {
    let p = |n: f64| (n * ratio).round_ties_even() as i64;
    let mut layout = Layout::default();
    let planet = window.contains("planet_entry") || window.contains("arkship_colony");
    if planet {
        match name {
            "icon" => layout.upper_right(-p(344.0), p(4.0)),
            "name" => layout.max_width = Some(p(150.0)),
            "colony_type" => {
                layout.upper_right(p(200.0) - width, p(2.0));
                layout.max_width = Some(p(150.0));
            }
            "planet_status_container" => {
                layout.position = Some((p(171.0), p(4.0)));
                layout.size = Some((p(104.0), p(21.0)));
                layout.orientation = Some("UPPER_LEFT");
            }
            "box" => layout.right_box(p(196.0) - width, p(2.0), p(159.0), p(26.0)),
            "progress" | "build_progress" => layout.upper_right(p(280.0) - width, p(9.0)),
            _ => (),
        }
    }
    let starbase = window.contains("starbase_entry");
    if starbase && name == "starbase_status_container" {
        layout.position = Some((p(213.0), p(4.0)));
        layout.size = Some((p(62.0), p(21.0)));
        layout.orientation = Some("UPPER_LEFT");
    }
    if starbase && !window.contains("arkship") {
        match name {
            "box" => layout.right_box(p(316.0) - width, p(2.0), p(150.0), p(26.0)),
            "military_power" => layout.upper_right(p(88.0) - width, p(2.0)),
            "name" => layout.max_width = Some(p(205.0)),
            _ => (),
        }
    }
    if window.contains("arkship_starbase") {
        match name {
            "constructions_ships" => {
                layout.position = Some((p(321.0), p(2.0)));
                layout.size = Some((p(184.0), p(26.0)));
            }
            "box" => layout.right_box(p(346.0) - width, p(2.0), p(159.0), p(26.0)),
            _ => (),
        }
    }
    if window.contains("shipyard_entry") && name == "constructions" {
        layout.position = Some((p(228.0), p(2.0)));
        layout.size = Some((p(277.0), p(26.0)));
    }
    if window.contains("fleet_entry") {
        match name {
            "fleet_status_grid" => layout.upper_right(p(343.0) - width, p(4.0)),
            "size_limit" => layout.upper_right(p(255.0) - width, p(2.0)),
            "offensive_power" => {
                layout.upper_right(p(485.0) - width, p(2.0));
                layout.max_width = Some(p(95.0));
            }
            "progress" => layout.upper_right(p(255.0) - width, p(9.0)),
            _ => (),
        }
    }
    if window == "outliner_member_army_entry_window" {
        match name {
            "name" => layout.max_width = Some(p(215.0)),
            "box" => {
                layout.position = Some((p(325.0), p(2.0)));
                layout.size = Some((p(180.0), p(26.0)));
                layout.format = Some("right");
            }
            "army_power" => layout.upper_right(p(95.0) - width, p(2.0)),
            _ => (),
        }
    }
    if window == "outliner_army_type_entry_window" && name == "amount" {
        layout.position = Some((p(24.0), p(2.0)));
        layout.max_width = Some(p(31.0));
        layout.orientation = Some("UPPER_LEFT");
        layout.format = Some("left");
    }
    if window.contains("megastructure") || window.contains("terraforming") {
        match name {
            "name" => layout.max_width = Some(p(205.0)),
            "system" | "star" => {
                layout.position = Some((width - p(85.0), p(2.0)));
                layout.max_width = Some(p(65.0));
                layout.orientation = Some("UPPER_LEFT");
            }
            "upgrade_icon" => layout.position = Some((p(252.0), p(4.0))),
            "type_target" => layout.position = Some((p(252.0), p(1.0))),
            "progress_bar" => layout.upper_right(p(280.0) - width, p(9.0)),
            _ => (),
        }
    }
    if window.contains("waystation_network") {
        match name {
            "waystation_count" => {
                layout.upper_right(-p(85.0), p(3.0));
                layout.max_width = Some(p(65.0));
            }
            "waystation_count_icon" => layout.upper_right(-p(100.0), p(3.0)),
            "stockpile_info" => layout.position = Some((p(250.0), p(3.0))),
            "stockpile_status" => {
                layout.position = Some((0, 0));
                layout.max_width = Some(p(80.0));
            }
            "stockpile_status_progressbar" => layout.position = Some((p(85.0), 0)),
            _ => (),
        }
    }
    if window.contains("situations_entry") {
        match name {
            "situations_progress_bar_container_short" => layout.position = Some((p(250.0), p(6.0))),
            "start_indicator" => layout.position = Some((p(250.0), p(7.0))),
            "middle_indicator" => layout.position = Some((p(347.0), p(3.0))),
            "pin_icon" => layout.position = Some((p(462.0), p(5.0))),
            "monthly_change" => {
                layout.position = Some((p(450.0), p(5.0)));
                layout.max_width = Some(p(55.0));
            }
            _ => (),
        }
    }
    layout
}

pub(super) fn apply_special(source: &str, width: i64, ratio: f64) -> Result<String> {
    fn visit(nodes: &[Node<'_>], window: &str, width: i64, ratio: f64, edits: &mut Edits) {
        for node in nodes {
            let name = node.named().unwrap_or_default();
            let window = if rows::is_entry(name) { name } else { window };
            if !window.is_empty() && name != window {
                let layout = special_layout(window, name, width, ratio);
                if let Some((x, y)) = layout.position {
                    xy(node, "position", x, y, edits);
                }
                if let Some((w, h)) = layout.size {
                    xy(node, "size", w, h, edits);
                }
                if let Some(w) = layout.max_width {
                    set(node, "maxWidth", w, edits);
                }
                if let Some(orientation) = layout.orientation {
                    set(
                        node,
                        "Orientation",
                        format_args!("\"{orientation}\""),
                        edits,
                    );
                }
                if let Some(format) = layout.format {
                    set(node, "format", format, edits);
                }
            }
            visit(node.children(), window, width, ratio, edits);
        }
    }
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    visit(&doc.nodes, "", width, ratio, &mut edits);
    edits.apply(source)
}
