use std::collections::BTreeMap;

use anyhow::Result;

use crate::{
    assets::Diagnostic,
    config::Scale,
    pdx::{Document, Edits, Node, Value},
};

const BLOCKS: &[&str] = &[
    "position",
    "show_position",
    "hide_position",
    "size",
    "bordersize",
    "slotsize",
    "maxsize",
    "minsize",
    "offset",
    "text_offset",
];
const SCALARS: &[&str] = &[
    "maxwidth",
    "maxheight",
    "max_width",
    "max_height",
    "min_width",
    "min_height",
    "spacing",
    "width",
    "height",
    "top",
    "bottom",
    "dynamic_extra_height",
];
const DEFINES: &[&str] = &[
    "PERSISTENT_CHAT_LINE_HEIGHT",
    "NOTIFICATION_MESSAGE_XPOS",
    "NOTIFICATION_MESSAGE_YPOS",
    "NOTIFICATION_MESSAGE_SPACING",
    "NOTIFICATION_MESSAGE_MARGIN_RIGHT",
    "ARCH_VIEW_TEXTLIST_OFFSET",
    "RIFT_VIEW_TEXTLIST_OFFSET",
    "COUNCIL_GRID_ARCH_HEIGHT",
    "SOCIALVIEW_CONTEXT_MENU_MARGIN",
    "SOCIALVIEW_CONTEXT_MENU_BUTTON_OFFSET",
    "MAP_ICON_ITEM_MINIMUM_WIDTH",
    "COSMIC_STORM_VISIBLE_GUI_FRAME_BOTTOM_PADDING",
    "COSMIC_STORM_VISIBLE_GUI_DESC_FRAME_BOTTOM_PADDING",
    "COSMIC_STORM_VISIBLE_GUI_CONFIRM_BUTTON_Y_OFFSET",
    "LEADER_ASSIGN_TRAIT_MIN_WIDTH",
    "SHIP_SELECT_ROLE_MIN_WIDTH",
    "SHIP_SELECT_ROLE_MIN_HEIGHT",
    "SHIP_SELECT_ROLE_HEIGHT_PADDING",
    "TRADE_VIEW_WIDE_MODE_THRESHHOLD",
];

pub fn scaled_atom(node: &Node<'_>, scale: Scale, edits: &mut Edits) {
    let Some(text) = node.atom() else {
        return;
    };
    let Ok(value) = text.parse::<f64>() else {
        return;
    };
    if !value.is_finite() || value == 0.0 {
        return;
    }
    let number = if value.fract() == 0.0 {
        format!("{:.0}", scale.pixels(value))
    } else {
        format!("{:.3}", value * scale.get())
    };
    let quoted = matches!(&node.value, Value::Atom { text, .. } if text.starts_with('"'));
    edits.atom(
        node,
        if quoted {
            format!("\"{number}\"")
        } else {
            number
        },
    );
}

/// # Errors
/// Возвращает ошибку синтаксиса или пересекающихся преобразований.
pub fn geometry(
    source: &str,
    scale: Scale,
    gfx: bool,
    path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<String> {
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    geometry_edits(&doc, scale, gfx, path, &mut edits, diagnostics);
    edits.apply(source)
}

#[allow(
    clippy::too_many_lines,
    reason = "Контекст размерностей передаётся одним рекурсивным проходом"
)]
pub(crate) fn geometry_edits(
    doc: &Document<'_>,
    scale: Scale,
    gfx: bool,
    path: &str,
    edits: &mut Edits,
    diagnostics: &mut Vec<Diagnostic>,
) {
    struct Context<'a> {
        scale: Scale,
        gfx: bool,
        path: &'a str,
    }
    #[derive(Clone, Copy, Default)]
    struct Scope {
        geometry: bool,
        resolution: bool,
        pie: bool,
    }
    fn visit(
        nodes: &[Node<'_>],
        context: &Context<'_>,
        scope: Scope,
        edits: &mut Edits,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for node in nodes {
            // Это число строк и столбцов, хотя движок читает его как positionType.
            if !context.gfx
                && context.path == "interface/ship_designer.gui"
                && node
                    .key
                    .is_some_and(|key| key.eq_ignore_ascii_case("positionType"))
                && node.named() == Some("min_component_slots")
            {
                continue;
            }
            let key = node.key.unwrap_or("");
            let is_key = |candidate: &str| key.eq_ignore_ascii_case(candidate);
            let in_keys =
                |candidates: &[&str]| candidates.iter().any(|candidate| is_key(candidate));
            let res = scope.resolution || is_key("if_scaled_resolution");
            let pixel = if context.gfx {
                in_keys(&[
                    "size",
                    "bg_position",
                    "bg_size",
                    "symbol_position",
                    "symbol_size",
                    "hitbox_margin",
                    "mask_size",
                    "mask_offset",
                    "position",
                ])
            } else {
                in_keys(BLOCKS)
            };
            if node.atom().is_some() {
                let dimension = scope.geometry && in_keys(&["x", "y", "width", "height"]);
                let scalar = !context.gfx
                    && in_keys(SCALARS)
                    && !(res && in_keys(&["min_height", "max_height", "min_width", "max_width"]));
                let variable =
                    !context.gfx && key.starts_with('@') && node.parse_atom::<i64>().is_some();
                if dimension || scalar || variable || (context.gfx && scope.pie && is_key("size")) {
                    if node.atom().is_some_and(|s| s.starts_with("@[")) {
                        diagnostics.push(Diagnostic {
                            path: context.path.into(),
                            span: Some(node.span.clone()),
                            rule: "expression-preserved".into(),
                            message: "Expression preserved; dimensional constants require review"
                                .into(),
                        });
                    }
                    scaled_atom(node, context.scale, edits);
                }
            }
            visit(
                node.children(),
                context,
                Scope {
                    geometry: pixel,
                    resolution: res,
                    pie: is_key("piecharttype"),
                },
                edits,
                diagnostics,
            );
        }
    }
    visit(
        &doc.nodes,
        &Context { scale, gfx, path },
        Scope::default(),
        edits,
        diagnostics,
    );
}

/// # Errors
/// Возвращает ошибку синтаксиса или правок выбранных констант.
pub fn defines(source: &str, scale: Scale) -> Result<String> {
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    doc.walk(|n| {
        if n.key.is_some_and(|k| DEFINES.contains(&k)) {
            scaled_atom(n, scale, &mut edits);
        }
    });
    edits.apply(source)
}

/// # Errors
/// Возвращает ошибку синтаксиса или пересекающихся правок.
pub fn widgets(source: &str, scale: Scale, statuses: &BTreeMap<String, bool>) -> Result<String> {
    let doc = Document::parse(source)?;
    let mut edits = Edits::default();
    widget_edits(
        &doc,
        scale,
        |sprite| statuses.get(sprite) == Some(&false),
        &mut edits,
    );
    edits.apply(source)
}

pub(crate) fn widget_edits(
    doc: &Document<'_>,
    scale: Scale,
    needs_scale: impl Fn(&str) -> bool,
    edits: &mut Edits,
) {
    widget_nodes(&doc.nodes, None, scale, &needs_scale, edits);
}

fn widget_nodes(
    nodes: &[Node<'_>],
    parent: Option<&str>,
    scale: Scale,
    needs_scale: &impl Fn(&str) -> bool,
    edits: &mut Edits,
) {
    for n in nodes {
        widget_nodes(n.children(), n.named(), scale, needs_scale, edits);
        if n.key == Some("instantTextBoxType") && n.field("maxWidth").is_none() {
            edits.insert(
                n,
                &format!("\n\t\tmaxWidth = {:.0}\n\t", scale.pixels(200.0)),
            );
        }
        if !n.key.is_some_and(|k| {
            [
                "iconType",
                "buttonType",
                "guiButtonType",
                "checkboxType",
                "effectbuttonType",
            ]
            .contains(&k)
        }) || n.field("size").is_some()
        {
            continue;
        }
        if n.named() == Some("3d_view") {
            continue;
        }
        let sprite = n
            .field("spriteType")
            .or_else(|| n.field("quadTextureSprite"))
            .and_then(Node::atom);
        if scaled_runtime_sprite(parent, n.named(), sprite) {
            continue;
        }
        if !sprite.is_some_and(needs_scale) {
            continue;
        }
        if let Some(sc) = n.field("scale") {
            if let Some(value) = sc.parse_atom::<f64>() {
                edits.atom(sc, format!("{:.4}", value * scale.get()));
            }
        } else {
            edits.insert(n, &format!("\n\t\tscale = {:.4}\n\t", scale.get()));
        }
    }
}

// Движок заменяет эти заглушки уже увеличенными спрайтами. Исходный scale
// сохраняется; статические виджеты с теми же заглушками требуют компенсации.
fn scaled_runtime_sprite(parent: Option<&str>, name: Option<&str>, sprite: Option<&str>) -> bool {
    match (name, sprite) {
        (_, Some("GFX_diplo_type_icon"))
        | (Some("large_resource_icon"), Some("gfx_transparency"))
        | (
            Some("astral_action_unlocked" | "relic_portrait"),
            Some("GFX_tiled_window_transparent"),
        )
        | (Some("passive_accord_icon"), Some("GFX_invisible_transparent"))
        | (
            Some(
                "specimen"
                | "specimen_icon"
                | "leader_rarity_frame"
                | "situation_icon"
                | "situation_icon_background"
                | "situation_entry_icon"
                | "situation_entry_icon_background"
                | "approach_icon"
                | "approach_icon_bg"
                | "stage_icon"
                | "stage_icon_background"
                | "outcome_left"
                | "outcome_left_frame"
                | "outcome_right"
                | "outcome_right_frame"
                | "planet_type_icon_overlay"
                | "designation_icon_overlay"
                | "covenant_icon"
                | "active_accord_button"
                | "not_eligible"
                | "portrait_overlay"
                | "dlc_button"
                | "dlc_frame"
                | "dlc_underlay",
            ),
            Some("GFX_invisible"),
        ) => true,
        (Some("frame"), Some("GFX_invisible")) => {
            parent == Some("specimen_acquired_custom_container")
        }
        (Some("background"), Some("GFX_pink_background")) => {
            parent.is_some_and(|parent| parent.starts_with("timeline_grid_"))
        }
        _ => false,
    }
}
