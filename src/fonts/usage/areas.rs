use crate::pdx::Node;

macro_rules! areas {
    ($($area:ident => $label:literal),+ $(,)?) => {
        /// Область интерфейса, распознанная по файлу, контексту элемента или известному имени группы.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum UsageArea { $($area),+ }

        impl UsageArea {
            pub const ALL: &[Self] = &[$(Self::$area),+];

            #[must_use]
            pub const fn label(self) -> &'static str {
                match self { $(Self::$area => $label),+ }
            }
        }
    };
}

areas! {
    MapResources => "Resources on the map",
    TopbarResources => "Top bar resources",
    MapShipCounters => "Ship counters on the map",
    MapFleetPower => "Fleet power on the map",
    MapObjectNames => "System, planet and object names on the map",
    MapEmpireNames => "Empire names across borders on the map",
    MapNebulaNames => "Nebula names on the map",
    MapSectorNames => "Sector names on the map",
    TopbarDateSpeed => "Date and game speed",
    PauseIndicator => "Pause indicator",
    SavingMessage => "Saving message",
    BottomSystemName => "System name in the bottom bar",
    BottomEmpireName => "Empire name in the bottom bar",
    MainMenuButtons => "Main menu buttons",
    LoadingStatus => "Loading progress text",
    LoadingTips => "Loading screen tips",
    Tooltips => "Tooltips and concept descriptions",
    MainMenu => "Main menu",
    LoadingScreen => "Loading screen",
    Topbar => "Top bar",
    BottomBar => "Bottom bar and control groups",
    MapIcons => "Map icons",
    GalaxyView => "Galaxy view",
    Outliner => "Outliner",
    PlanetView => "Planet view",
    ShipDesigner => "Ship designer",
    Technology => "Technology",
    Events => "Event windows",
    Notifications => "Notifications and alerts",
    Dialogs => "Dialogs and confirmations",
    Diplomacy => "Diplomacy and agreements",
    Chat => "Chat",
    Multiplayer => "Multiplayer and lobbies",
    GameSetup => "New game and empire creation",
    SaveLoad => "Save and load menus",
    Settings => "Settings",
    AdditionalContent => "DLC and additional content",
    Credits => "Credits",
    EndScreen => "End game screen",
    Tutorial => "Tutorial and advisor",
    Species => "Species and species rights",
    Leaders => "Leaders",
    Government => "Government and council",
    Traditions => "Traditions and ascension perks",
    Market => "Market and automatic trades",
    Fleets => "Fleets and fleet manager",
    Starbases => "Starbases and stations",
    Ships => "Ship details",
    Situations => "Situation log and exploration",
    Console => "Console and debug text",
}

impl UsageArea {
    pub(super) fn in_file(path: &str) -> Option<Self> {
        let file = path.strip_prefix("interface/")?;
        Some(match file {
            "mapicons.gui" => Self::MapIcons,
            "galaxy_view.gui" => Self::GalaxyView,
            "outliner.gui" => Self::Outliner,
            "planet_view.gui" | "synaptic_lathe_view.gui" => Self::PlanetView,
            "ship_designer.gui" | "ship_designer_role_selection.gui" | "shipdesign_dialog.gui" => {
                Self::ShipDesigner
            }
            "technology_view.gui" => Self::Technology,
            "frontend.gui" | "mainmenu_view.gui" => Self::MainMenu,
            "main_bottom.gui" => Self::BottomBar,
            "load_screen.gui" => Self::LoadingScreen,
            "eventwindow.gui"
            | "leader_story_event_window.gui"
            | "leader_conversation_event_window.gui"
            | "leader_recruitment_event_window.gui"
            | "crisis_conversation_event_window.gui" => Self::Events,
            "alerts.gui" | "startup_info_view.gui" | "message_type_settings_view.gui" => {
                Self::Notifications
            }
            "popup.gui" | "standard_dialog.gui" | "rename_dialog.gui" => Self::Dialogs,
            "trade_view.gui"
            | "agreement_negotiation_view.gui"
            | "federation_view.gui"
            | "galactic_community_view.gui"
            | "war_goals_view.gui"
            | "waroverview.gui"
            | "alliance_view.gui" => Self::Diplomacy,
            "chat.gui" | "social_view.gui" | "matchmaking_chat.gui" => Self::Chat,
            "matchmaking.gui"
            | "matchmaking_coop.gui"
            | "multiplayer_view.gui"
            | "ingamelobby.gui"
            | "ingame_mp_lobby.gui"
            | "mp_gamesetup.gui"
            | "mp_country_settings_view.gui"
            | "mp_country_password_view.gui"
            | "hotjoin.gui"
            | "hotjoin_view.gui" => Self::Multiplayer,
            "new_game_setup.gui"
            | "select_empire_design.gui"
            | "customize_species.gui"
            | "customize_species_editors.gui"
            | "customize_species_shipsets.gui"
            | "start_screen.gui"
            | "species_traits.gui"
            | "species_weapons_and_travel.gui" => Self::GameSetup,
            "savegame_view.gui" | "loadgame_view.gui" => Self::SaveLoad,
            "settings_view.gui" => Self::Settings,
            "credits.gui" => Self::Credits,
            "endscreen.gui" => Self::EndScreen,
            "tutorial_mission_window.gui"
            | "tutorial_mission_window_docked.gui"
            | "hud_tutorial.gui"
            | "immediate_tutorial.gui"
            | "advisor_window.gui" => Self::Tutorial,
            "topbar_species_view.gui"
            | "species_mod_window.gui"
            | "species_mod_apply_window.gui" => Self::Species,
            "topbar_leaders_view.gui" | "leader_view.gui" | "leader_pool_view.gui" => Self::Leaders,
            "government_view.gui" | "government_mod_window.gui" | "council_view.gui" => {
                Self::Government
            }
            "topbar_traditions_view.gui" | "ascension_perks_view.gui" => Self::Traditions,
            "market_view.gui" | "monthly_trade_view.gui" => Self::Market,
            "fleet_view.gui"
            | "fleet_manager_view.gui"
            | "fleet_action_view.gui"
            | "home_base_view.gui" => Self::Fleets,
            "starbase_view.gui"
            | "station_selection.gui"
            | "waystation_view.gui"
            | "megastructure_view.gui" => Self::Starbases,
            "ship_view.gui" | "ship_design_view.gui" => Self::Ships,
            "situation_log.gui"
            | "anomaly_view.gui"
            | "archaeology_view.gui"
            | "astral_rift_view.gui" => Self::Situations,
            "console.gui" => Self::Console,
            _ if file.starts_with("game_setup/") => Self::GameSetup,
            _ if file.starts_with("additional_content/") => Self::AdditionalContent,
            _ if file.starts_with("diplomacy_") => Self::Diplomacy,
            _ => return None,
        })
    }

    pub(super) fn in_block(path: &str, block: &Node<'_>, inherited: Option<Self>) -> Option<Self> {
        let name = block.named().unwrap_or_default();
        Some(match (path, name) {
            ("interface/mapicons.gui", "resource_icon_item") => Self::MapResources,
            ("interface/mapicons.gui", "ship_counter_entry") => Self::MapShipCounters,
            ("interface/mapicons.gui", "offensive_power") => Self::MapFleetPower,
            (
                "interface/mapicons.gui",
                "planet_map_icon"
                | "ambient_object_mapicon"
                | "megastructure_map_icon"
                | "star_mapicon"
                | "natural_wormhole_map_icon"
                | "astral_rift_map_icon",
            ) => Self::MapObjectNames,
            ("interface/main.gui", "topbar_static" | "topbar_extended") => Self::TopbarResources,
            ("interface/main.gui", "topbar") => Self::Topbar,
            ("interface/main.gui", "saving_window") => Self::SavingMessage,
            ("interface/main.gui", "time_window") => Self::TopbarDateSpeed,
            ("interface/main_bottom.gui", "paused") => Self::PauseIndicator,
            ("interface/main_bottom.gui", "system_name") => Self::BottomSystemName,
            ("interface/main_bottom.gui", "empire_name") => Self::BottomEmpireName,
            ("interface/frontend.gui", "buttons") => Self::MainMenuButtons,
            ("interface/load_screen.gui", "status") => Self::LoadingStatus,
            ("interface/load_screen.gui", "tip") => Self::LoadingTips,
            ("interface/core.gui", "ToolTipWindow" | "ToolTipConceptWindow") => Self::Tooltips,
            ("interface/main.gui", "debug_stats")
            | ("interface/core.gui", "console_edit" | "ConsoleText" | "GuiBounds") => Self::Console,
            _ => return inherited,
        })
    }

    /// Предполагаемые роли штатных групп без явных назначений в GUI.
    pub(super) fn in_group(name: &str) -> Option<Self> {
        match name {
            "map_name_border" => Some(Self::MapEmpireNames),
            "map_name_nebula" => Some(Self::MapNebulaNames),
            "map_name_sector" => Some(Self::MapSectorNames),
            _ => None,
        }
    }
}
