use anyhow::Result;

use crate::{
    config::Scale,
    pdx::{Document, Edits, Node},
};

#[derive(Clone, Copy)]
enum Anchor {
    UpperRight,
    LowerLeft,
    LowerRight,
}

impl Anchor {
    fn parse(value: &str) -> Option<Self> {
        [
            ("UPPER_RIGHT", Self::UpperRight),
            ("LOWER_LEFT", Self::LowerLeft),
            ("LOWER_RIGHT", Self::LowerRight),
        ]
        .into_iter()
        .find_map(|(name, anchor)| value.eq_ignore_ascii_case(name).then_some(anchor))
    }

    const fn offsets(self, dx: f64, dy: f64) -> [(&'static str, f64); 2] {
        let (x, y) = match self {
            Self::UpperRight => (dx, 0.0),
            Self::LowerLeft => (0.0, dy),
            Self::LowerRight => (dx, dy),
        };
        [("x", x), ("y", y)]
    }
}

pub(super) fn trait_buttons(source: &str, scale: Scale) -> Result<String> {
    // UpdateBottomContainerPosition перезаписывает позиции панели, OK и крестика.
    // Внешние контейнеры добавляют недостающий масштаб фиксированных отступов.
    let bottom_offset = (15.0 - scale.pixels(15.0), scale.pixels(50.0) - 50.0);
    let close_offset = (41.0 - scale.pixels(41.0), scale.pixels(9.0) - 9.0);
    if bottom_offset == (0.0, 0.0) && close_offset == (0.0, 0.0) {
        return Ok(source.into());
    }
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    doc.walk(|node| {
        if node.named() != Some("assign_trait_view") {
            return;
        }
        for (name, wrapper, (x, y)) in [
            (
                "bottom_container",
                "ui_scaler_trait_bottom_offset",
                bottom_offset,
            ),
            ("close_button", "ui_scaler_trait_close_offset", close_offset),
        ] {
            if let Some(child) = node.named_child(name) {
                offset_wrapper(source, child, wrapper, x, y, &mut edits);
            }
        }
    });
    edits.apply(source)
}

fn offset_wrapper(source: &str, node: &Node<'_>, name: &str, x: f64, y: f64, edits: &mut Edits) {
    edits.replace(
        node.span.clone(),
        format!(
            "containerWindowType = {{\n\t\t\tname = \"{name}\"\n\t\t\tposition = {{ x = {x:.0} y = {y:.0} }}\n\t\t\tsize = {{ width = 100% height = 100% }}\n\t\t\tclipping = no\n\t\t\t{}\n\t\t}}",
            &source[node.span.clone()],
        ),
    );
}

pub(super) fn navbar(source: &str, scale: Scale) -> Result<String> {
    // Движок каждый кадр заменяет position контейнера background на (-60, 5).
    // Вложенный фон добавляет недостающий масштаб смещения, сохраняя геометрию текстуры.
    let x = scale.pixels(-60.0) + 60.0;
    let y = scale.pixels(5.0) - 5.0;
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    doc.walk(|node| {
        if node.named() != Some("maingui_left_bar_buttons") {
            return;
        }
        if let Some(background) = node
            .named_child("background")
            .and_then(|child| child.field("background"))
        {
            offset_wrapper(
                source,
                background,
                "ui_scaler_navbar_background",
                x,
                y,
                &mut edits,
            );
        }
        navbar_lock_anchor(node, scale, &mut edits);
    });
    edits.apply(source)
}

fn navbar_lock_anchor(panel: &Node<'_>, scale: Scale, edits: &mut Edits) {
    // Setup ищет lock через нерекурсивный GetButton; кнопку нельзя вкладывать в обёртку.
    // UpdateLeftBarButtons задаёт (-20, -112). Сдвигаем её LOWER_RIGHT через размер панели.
    let dx = scale.pixels(-20.0) + 20.0;
    let dy = scale.pixels(-112.0) + 112.0;
    if (dx == 0.0 && dy == 0.0) || panel.named_child("lock").is_none() {
        return;
    }
    let Some(size) = panel.field("size") else {
        return;
    };
    let (Some(width), Some(height)) = (size.field("width"), size.field("height")) else {
        return;
    };
    let (Some(w), Some(h)) = (width.parse_atom::<f64>(), height.parse_atom::<f64>()) else {
        return;
    };
    edits.atom(width, format!("{:.0}", w + dx));
    edits.atom(height, format!("{:.0}", h + dy));
    // Остальные кнопки сохраняют экранные координаты при изменении своего якоря.
    for child in panel
        .children()
        .iter()
        .filter(|child| child.named() != Some("lock"))
    {
        let Some(anchor) = child
            .field("orientation")
            .and_then(Node::atom)
            .and_then(Anchor::parse)
        else {
            continue;
        };
        let Some(position) = child.field("position") else {
            continue;
        };
        for (axis, delta) in anchor.offsets(dx, dy) {
            if delta != 0.0
                && let Some(value) = position.field(axis)
                && let Some(number) = value.parse_atom::<f64>()
            {
                edits.atom(value, format!("{:.0}", number - delta));
            }
        }
    }
}
