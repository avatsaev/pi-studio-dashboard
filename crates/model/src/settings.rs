//! Settings mirror: filled from `Config` by the engine; defaults match the config defaults.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
    #[default]
    System,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NotificationPrefs {
    pub needs_input: bool,
    pub failures: bool,
}

impl Default for NotificationPrefs {
    fn default() -> Self {
        Self { needs_input: true, failures: true }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SettingsMirror {
    pub theme: Theme,
    pub notifications: NotificationPrefs,
    pub close_to_tray: bool,
    pub attention_rail_collapsed: bool,
}

impl Default for SettingsMirror {
    fn default() -> Self {
        Self { theme: Theme::System, notifications: NotificationPrefs::default(), close_to_tray: true, attention_rail_collapsed: false }
    }
}
