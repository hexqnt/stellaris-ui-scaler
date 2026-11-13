use anyhow::{Result, anyhow};
use eframe::egui;

macro_rules! icons {
    ($($name:ident, $glyph:literal, $file:literal);+ $(;)?) => {
        $(pub(super) const $name: &str = $glyph;)+

        pub(super) fn install(ctx: &egui::Context) -> Result<()> {
            $(
                let glyph = egui_extras::SvgGlyph::from_bytes(include_bytes!(concat!("../../assets/icons/", $file, ".svg")))
                    .map_err(|error| anyhow!("Cannot load embedded icon {}: {error}", $file))?
                    .with_height(1.1)
                    .with_baseline_offset(-0.15);
                ctx.add_glyph_rasterizer(glyph.into_rasterizer($glyph));
            )+
            Ok(())
        }
    };
}

icons! {
    LOGO, "\u{e000}", "aperture";
    PROJECT, "\u{e001}", "folder";
    TEXTURES, "\u{e002}", "photo";
    FONTS, "\u{e003}", "typography";
    MOD, "\u{e004}", "tag";
    SAVE, "\u{e005}", "device-floppy";
    BUILD, "\u{e006}", "hammer";
    INSTALL, "\u{e007}", "download";
    SEARCH, "\u{e008}", "search";
    SPLIT, "\u{e009}", "arrows-horizontal";
    COMPARE, "\u{e00a}", "layout-grid";
    AB, "\u{e00b}", "arrows-diff";
    PREVIOUS, "\u{e00c}", "chevron-left";
    NEXT, "\u{e00d}", "chevron-right";
    ZOOM_IN, "\u{e00e}", "zoom-in";
    ZOOM_OUT, "\u{e00f}", "zoom-out";
    READY, "\u{e010}", "circle-check";
    SCAN, "\u{e011}", "scan";
    FILE, "\u{e012}", "file";
    CLEAR, "\u{e013}", "x";
    LOAD, "\u{e014}", "folder-open";
    FIT, "\u{e015}", "arrows-maximize";
    RESET, "\u{e016}", "rotate-2";
}

pub(super) fn button(icon: &str, label: &str) -> egui::Button<'static> {
    egui::Button::new((
        egui::RichText::new(icon).size(17.0),
        egui::RichText::new(label),
    ))
    .gap(7.0)
}
