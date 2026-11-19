use crate::fonts::Group;

use super::*;

#[test]
fn specialized_areas_cover_menu_loading_status_messages_and_bottom_bar_names() {
    let mut index = Index::default();
    let cases = [
        (
            "interface/frontend.gui",
            "buttons",
            "text",
            "jura",
            UsageArea::MainMenuButtons,
        ),
        (
            "interface/load_screen.gui",
            "status",
            "text",
            "load_screen",
            UsageArea::LoadingStatus,
        ),
        (
            "interface/load_screen.gui",
            "tip",
            "text",
            "load_screen",
            UsageArea::LoadingTips,
        ),
        (
            "interface/main.gui",
            "saving_window",
            "text",
            "large_title_font",
            UsageArea::SavingMessage,
        ),
        (
            "interface/main.gui",
            "time_window",
            "time",
            "cg_16b",
            UsageArea::TopbarDateSpeed,
        ),
        (
            "interface/main_bottom.gui",
            "leave_system_window",
            "paused",
            "large_title_font",
            UsageArea::PauseIndicator,
        ),
        (
            "interface/main_bottom.gui",
            "leave_system_window",
            "system_name",
            "large_title_font_28",
            UsageArea::BottomSystemName,
        ),
        (
            "interface/main_bottom.gui",
            "leave_galaxy_window",
            "empire_name",
            "large_title_font_28",
            UsageArea::BottomEmpireName,
        ),
        (
            "interface/core.gui",
            "ToolTipConceptWindow",
            "text",
            "cg_16b",
            UsageArea::Tooltips,
        ),
    ];
    let mut catalog = Catalog::default();
    for (file, parent, widget, font, _) in cases {
        let source = format!(
            "windowType = {{ name = {parent} textBoxType = {{ name = {widget} font = {font} }} }}"
        );
        index.read_gui(file, &Document::parse(&source).unwrap());
        if !catalog.groups.iter().any(|group| group.name == font) {
            catalog.groups.push(Group {
                name: font.into(),
                ..Default::default()
            });
        }
    }
    index.apply(&mut catalog);
    for (_, _, widget, font, area) in cases {
        let group = catalog
            .groups
            .iter()
            .find(|group| group.name == font)
            .unwrap();
        assert!(
            group
                .usages
                .iter()
                .any(|usage| usage.widget.as_deref() == Some(widget) && usage.area == Some(area)),
            "{font}: {area:?}"
        );
        assert!(group.uses_area(area));
        assert!(group.usage_note().is_some());
    }
}

#[test]
fn group_name_hints_are_separate_from_observed_gui_assignments() {
    for (name, area) in [
        ("map_name_border", UsageArea::MapEmpireNames),
        ("map_name_nebula", UsageArea::MapNebulaNames),
        ("map_name_sector", UsageArea::MapSectorNames),
    ] {
        let group = Group {
            name: name.into(),
            ..Default::default()
        };
        assert!(group.uses_area(area));
        assert_eq!(group.usage_areas().collect::<Vec<_>>(), [area]);
        assert!(group.usages.is_empty());
        assert!(group.usage_note().unwrap().contains("inferred"));
    }
    for name in ["standard_font", "null_font", "custom_mod_font"] {
        let group = Group {
            name: name.into(),
            ..Default::default()
        };
        assert!(group.usage_areas().next().is_none());
    }
    let group = Group {
        name: "custom_mod_font".into(),
        ..Default::default()
    };
    assert!(group.usage_note().is_none());
}

#[test]
fn areas_follow_widget_ancestors_without_leaking_to_siblings_or_unknown_files() {
    let mut index = Index::default();
    let map = Document::parse(
        r"guiTypes = {
        windowType = { name = resource_icon_item
            instantTextBoxType = { name = num font = shared }
        }
        windowType = { name = ship_counter_entry
            instantTextBoxType = { name = count font = shared }
        }
        containerWindowType = { name = fleet_map_icon
            instantTextBoxType = { name = offensive_power font = shared }
            instantTextBoxType = { name = other font = shared }
        }
    }",
    )
    .unwrap();
    index.read_gui("interface/mapicons.gui", &map);
    let topbar = Document::parse(
        r"guiTypes = {
        containerWindowType = { name = topbar_static
            containerWindowType = { name = tb_energy_group
                instantTextBoxType = { name = amount font = shared }
            }
        }
        instantTextBoxType = { name = date font = shared }
    }",
    )
    .unwrap();
    index.read_gui("interface/main.gui", &topbar);
    index.read_gui("interface/mod_unknown.gui", &map);
    let mut catalog = Catalog {
        groups: vec![Group {
            name: "shared".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    index.apply(&mut catalog);
    let usages = &catalog.groups[0].usages;
    assert_eq!(
        usages
            .iter()
            .filter(|u| u.file.as_ref() == "interface/mapicons.gui")
            .map(|u| u.area)
            .collect::<Vec<_>>(),
        [
            Some(UsageArea::MapResources),
            Some(UsageArea::MapShipCounters),
            Some(UsageArea::MapFleetPower),
            Some(UsageArea::MapIcons),
        ]
    );
    assert_eq!(
        usages
            .iter()
            .filter(|u| u.file.as_ref() == "interface/main.gui")
            .map(|u| u.area)
            .collect::<Vec<_>>(),
        [Some(UsageArea::TopbarResources), None]
    );
    assert!(
        usages
            .iter()
            .filter(|u| u.file.as_ref() == "interface/mod_unknown.gui")
            .all(|u| u.area.is_none())
    );
}
