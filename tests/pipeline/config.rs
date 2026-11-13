use super::*;

#[test]
fn font_style_preset_resets_typefaces_keeps_sizes_and_roundtrips() {
    use stellaris_ui_scaler::config::{FontMode, FontRule, FontSize};
    let mut profile = Profile::default();
    profile.fonts.insert(
        "cg_16b".into(),
        FontRule {
            replace_bitmap: Some(false),
            family: Some("Custom".into()),
            scale: Some(Scale::try_from(1.25).unwrap()),
            base_size: Some(FontSize::try_from(17.0).unwrap()),
        },
    );
    profile.fonts.insert(
        "hoi_16mbs".into(),
        FontRule {
            replace_bitmap: Some(false),
            ..Default::default()
        },
    );
    profile.apply_font_mode(FontMode::TrueType);
    assert_eq!(profile.font_mode, FontMode::TrueType);
    assert_eq!(profile.fonts.len(), 1);
    assert_eq!(profile.fonts["cg_16b"].family, None);
    assert_eq!(profile.fonts["cg_16b"].replace_bitmap, None);
    assert_eq!(profile.fonts["cg_16b"].scale.unwrap().get(), 1.25);
    assert_eq!(f64::from(profile.fonts["cg_16b"].base_size.unwrap()), 17.0);
    let serialized = toml::to_string(&profile).unwrap();
    assert!(serialized.contains("font_mode = \"truetype\""));
    let restored: Profile = toml::from_str(&serialized).unwrap();
    assert_eq!(restored.font_mode, profile.font_mode);
    assert_eq!(restored.fonts, profile.fonts);
    assert_eq!(
        toml::from_str::<Profile>("").unwrap().font_mode,
        FontMode::Automatic
    );
    assert!(toml::from_str::<Profile>("font_mode = 'unknown'").is_err());
}

#[test]
fn fixed_auto_choices_skip_pixel_analysis() {
    let path = "gfx/test.dds";
    let mut profile = Profile {
        auto_algorithm: true,
        auto_sharpening: true,
        scale: Scale::try_from(2.0).unwrap(),
        ..Profile::default()
    };
    assert!(!profile.needs_pixel_analysis(path, Category::Mask, Some((64, 64))));
    assert!(profile.needs_pixel_analysis(path, Category::Icon, Some((64, 64))));
    profile.scale = Scale::try_from(1.0).unwrap();
    assert!(!profile.needs_pixel_analysis(path, Category::Icon, Some((64, 64))));
    profile.scale = Scale::try_from(0.5).unwrap();
    // При уменьшении фильтр известен заранее, но Auto sharpening ещё может зависеть от границ.
    assert!(profile.needs_pixel_analysis(path, Category::Icon, Some((64, 64))));
    profile.auto_sharpening = false;
    assert!(!profile.needs_pixel_analysis(path, Category::Icon, Some((64, 64))));
    profile.auto_algorithm = false;
    profile.auto_sharpening = true;
    for algorithm in [Algorithm::Bilinear, Algorithm::Nearest, Algorithm::Mitchell] {
        profile.textures.algorithm = algorithm;
        assert!(!profile.needs_pixel_analysis(path, Category::Icon, Some((64, 64))));
    }
    profile.textures.algorithm = Algorithm::CatmullRom;
    profile.textures.sharpening = 0.05;
    assert!(!profile.needs_pixel_analysis(path, Category::Icon, Some((64, 64))));
}

#[test]
fn auto_sharpening_is_independent_and_preserves_explicit_amounts() {
    let mut profile = Profile {
        auto_sharpening: true,
        ..Profile::default()
    };
    let path = "gfx/test.dds";
    profile.textures.algorithm = Algorithm::Mitchell;
    assert_eq!(profile.options(path, Category::Icon).sharpening, 0.15);
    profile.textures.sharpening = 0.05;
    assert_eq!(profile.options(path, Category::Icon).sharpening, 0.05);
    profile.textures.algorithm = Algorithm::Bilinear;
    assert_eq!(profile.options(path, Category::Icon).sharpening, 0.0);
    profile.directories.insert(
        "gfx".into(),
        TextureRule {
            sharpening: Some(0.7),
            ..Default::default()
        },
    );
    assert_eq!(profile.options(path, Category::Icon).sharpening, 0.7);
    assert!(!profile.needs_pixel_analysis(path, Category::Icon, None));
    profile.directories.clear();
    profile.auto_sharpening = false;
    assert_eq!(profile.options(path, Category::Icon).sharpening, 0.05);
}

#[test]
fn scale_and_profiles_encode_invariants() {
    for invalid in [f64::NAN, f64::INFINITY, 0.0, -1.0, 9.0] {
        assert!(Scale::try_from(invalid).is_err());
    }
    assert_eq!(Scale::try_from(1.5).unwrap().pixels(3.0), 4.0);
    assert_eq!(Scale::try_from(1.5).unwrap().pixels(-3.0), -4.0);
    let mut profile = Profile::default();
    profile.categories.insert(
        "icon".into(),
        TextureRule {
            algorithm: Some(Algorithm::Mitchell),
            ..Default::default()
        },
    );
    profile.assets.insert(
        "gfx/a.png".into(),
        TextureRule {
            algorithm: Some(Algorithm::Nearest),
            ..Default::default()
        },
    );
    assert_eq!(
        profile.options("gfx/a.png", Category::Icon).algorithm,
        Algorithm::Nearest
    );
    assert_eq!(
        profile.options("gfx/b.png", Category::Icon).algorithm,
        Algorithm::Mitchell
    );
    assert!(safe_relative("../bad").is_err());
    assert!(safe_relative("/bad").is_err());
}

#[test]
fn invalid_mod_versions_are_rejected() {
    for version in ["", "  ", "1\".0", "1\\0", "1\n0", "1\r0", "1\t0", "1\u{0}0"] {
        let profile = Profile {
            mod_version: version.into(),
            ..Profile::default()
        };
        assert!(profile.check().is_err(), "{version:?}");
        let profile = Profile {
            supported_version: Some(version.into()),
            ..Profile::default()
        };
        assert!(profile.check().is_err(), "{version:?}");
    }
}

#[test]
fn invalid_mod_names_are_rejected_without_overwriting_a_saved_profile() {
    let temp = Temp::new();
    let path = temp.0.join("profile.toml");
    let mut profile = Profile::default();
    profile.save(&path).unwrap();
    let saved = fs::read(&path).unwrap();
    for name in [
        "",
        "  ",
        "Mod\"name",
        "Mod\\name",
        "Mod\nname",
        "Mod\tname",
        "Mod\0name",
    ] {
        profile.name = name.into();
        assert!(profile.save(&path).is_err(), "{name:?}");
        assert_eq!(fs::read(&path).unwrap(), saved);
    }
    profile.name = "Updated UI".into();
    profile.save(&path).unwrap();
    assert_eq!(Profile::load(&path).unwrap().name, profile.name);
    assert_eq!(stellaris_ui_scaler::assets::files(&temp.0).unwrap(), [path]);
}

#[test]
fn texture_rules_inherit_each_field_through_directory_levels() {
    let mut profile: Profile = toml::from_str(
        r#"
scale = 2.0
[categories.icon]
algorithm = "mitchell"
premultiplied_alpha = false
sharpening_threshold = 9
[directories.gfx]
color_space = "linear"
sharpening_radius = 2.0
[directories."gfx/interface"]
algorithm = "nearest"
scale = 1.5
[directories."gfx/interface/buttons"]
sharpening = 0.0
independent_frames = false
[assets."gfx/interface/buttons/a.dds"]
algorithm = "bilinear"
premultiplied_alpha = true
sharpening_threshold = 0
"#,
    )
    .unwrap();
    profile.check().unwrap();
    let path = "gfx/interface/buttons/a.dds";
    let options = profile.options(path, Category::Icon);
    assert_eq!(
        options,
        TextureOptions {
            algorithm: Algorithm::Bilinear,
            color_space: ColorSpace::Linear,
            premultiplied_alpha: true,
            independent_frames: false,
            sharpening: 0.0,
            sharpening_radius: 2.0,
            sharpening_threshold: 0,
        }
    );
    assert_eq!(profile.texture_scale(path, Category::Icon).get(), 1.5);
    assert_eq!(
        profile
            .options("gfx/interface/buttons/b.dds", Category::Icon)
            .algorithm,
        Algorithm::Nearest
    );
    assert_eq!(
        profile
            .options("gfx/interfaces/b.dds", Category::Icon)
            .algorithm,
        Algorithm::Mitchell
    );
    assert_eq!(
        profile
            .texture_scale("gfx/interfaces/b.dds", Category::Icon)
            .get(),
        2.0
    );
    assert_eq!(
        profile.options("other.dds", Category::Artwork),
        profile.textures
    );
    profile.assets.get_mut(path).unwrap().algorithm = None;
    assert_eq!(
        profile.options(path, Category::Icon).algorithm,
        Algorithm::Nearest
    );
    profile.directories.remove("gfx/interface");
    assert_eq!(
        profile.options(path, Category::Icon).algorithm,
        Algorithm::Mitchell
    );
    let serialized = toml::to_string(&profile).unwrap();
    let restored: Profile = toml::from_str(&serialized).unwrap();
    assert_eq!(
        restored.options(path, Category::Icon),
        profile.options(path, Category::Icon)
    );
    for key in ["../gfx", "/gfx"] {
        profile
            .directories
            .insert(key.into(), TextureRule::default());
        assert!(profile.check().is_err());
        profile.directories.remove(key);
    }
    profile.directories.insert(
        "gfx".into(),
        TextureRule {
            sharpening_radius: Some(f32::NAN),
            ..Default::default()
        },
    );
    assert!(profile.check().is_err());
}

#[test]
fn auto_algorithm_respects_manual_rules_and_round_trips() {
    let mut profile = Profile {
        auto_algorithm: true,
        auto_sharpening: true,
        ..Profile::default()
    };
    let path = "gfx/icons/test_mask.dds";
    assert_eq!(
        profile.options(path, Category::Icon).algorithm,
        Algorithm::Bilinear
    );
    for (map, key, algorithm) in [
        (&mut profile.categories, "icon", Algorithm::Mitchell),
        (&mut profile.directories, "gfx", Algorithm::Nearest),
        (&mut profile.assets, path, Algorithm::Lanczos2),
    ] {
        map.insert(
            key.into(),
            TextureRule {
                algorithm: Some(algorithm),
                ..Default::default()
            },
        );
    }
    assert_eq!(
        profile.options(path, Category::Icon).algorithm,
        Algorithm::Lanczos2
    );
    assert!(
        profile
            .automatic_algorithm(path, Category::Icon, None)
            .is_none()
    );
    profile.assets.clear();
    assert_eq!(
        profile.options(path, Category::Icon).algorithm,
        Algorithm::Nearest
    );
    profile.directories.clear();
    assert_eq!(
        profile.options(path, Category::Icon).algorithm,
        Algorithm::Mitchell
    );
    profile.categories.clear();
    let restored: Profile = toml::from_str(&toml::to_string(&profile).unwrap()).unwrap();
    assert!(restored.auto_algorithm);
    assert!(restored.auto_sharpening);
    assert_eq!(
        restored.options(path, Category::Icon).algorithm,
        Algorithm::Bilinear
    );
    profile.auto_algorithm = false;
    assert_eq!(
        profile.options(path, Category::Icon).algorithm,
        profile.textures.algorithm
    );
    assert_eq!(
        profile.options(path, Category::Icon).sharpening,
        profile.textures.sharpening
    );
}
