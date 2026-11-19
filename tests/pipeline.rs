#[path = "pipeline/assets.rs"]
mod assets_tests;
#[path = "pipeline/cancellation.rs"]
mod cancellation_tests;
#[path = "pipeline/cli.rs"]
mod cli_tests;
#[path = "pipeline/config.rs"]
mod config_tests;
#[path = "pipeline/discovery.rs"]
mod discovery_tests;
#[path = "pipeline/engine.rs"]
mod engine_tests;
#[path = "pipeline/font_styles.rs"]
mod font_styles_tests;
#[path = "pipeline/fonts.rs"]
mod fonts_tests;
#[path = "pipeline/geometry.rs"]
mod geometry_tests;
#[path = "pipeline/layout.rs"]
mod layout_tests;
#[path = "pipeline/neural.rs"]
mod neural_tests;
#[path = "pipeline/output.rs"]
mod output_tests;
#[path = "pipeline/pdx.rs"]
mod pdx_tests;
#[path = "pipeline/preview.rs"]
mod preview_tests;
#[path = "pipeline/source_mods.rs"]
mod source_mods_tests;
#[path = "support/temp.rs"]
mod temp;
#[path = "support/images.rs"]
mod test_images;
#[path = "pipeline/textures.rs"]
mod textures_tests;

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use stellaris_ui_scaler::{
    assets::{Installation, safe_relative},
    compiler::{self, Control},
    config::{Algorithm, Category, ColorSpace, Profile, Scale, TextureOptions, TextureRule},
    fonts, outliner,
    pdx::{Document, Edits, Node},
    scaling, textures, validation,
};

use temp::Temp;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn named<'node, 'source>(node: &'node Node<'source>, name: &str) -> &'node Node<'source> {
    node.named_child(name).unwrap()
}

fn number(node: &Node<'_>, block: &str, axis: &str) -> f64 {
    node.field(block)
        .unwrap()
        .field(axis)
        .unwrap()
        .parse_atom()
        .unwrap()
}

fn fixture() -> Temp {
    let temp = Temp::new();
    let root = temp.0.join("game");
    write(
        &root,
        "launcher-settings.json",
        r#"{"rawVersion":"v4.5.2"}"#,
    );
    write(
        &root,
        "interface/example.gfx",
        r#"spriteTypes = { spriteType = { name = "GFX_test" textureFile = "gfx/test.png" noOfFrames = 2 } }"#,
    );
    write(
        &root,
        "interface/example.gui",
        r#"guiTypes = { containerWindowType = { name = "sample" size = { width = 80 height = 40 } iconType = { name = "icon" spriteType = "GFX_test" position { x = -4 y = 3 } } instantTextBoxType = { name = "label" text = "Текст" } } }"#,
    );
    fs::create_dir_all(root.join("gfx")).unwrap();
    let image = image::RgbaImage::from_fn(8, 4, |x, _| {
        if x < 4 {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0, 255, 0, 255])
        }
    });
    image.save(root.join("gfx/test.png")).unwrap();
    temp
}
