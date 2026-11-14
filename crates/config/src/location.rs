//! Where the config directory lives (native only). The app and the CLI share this rule so
//! `pistudiodashboard-cli config path` names the file the app uses.

use std::path::PathBuf;

/// The app's bundle identifier; also the name of its config directory (what Tauri's
/// `app_config_dir` resolves to).
pub const APP_IDENTIFIER: &str = "com.avatsaev.pistudiodashboard";

/// `<platform config dir>/com.avatsaev.pistudiodashboard`, or `None` when the environment gives
/// no home. `env` looks a variable up (injected for tests).
///
/// Linux: `$XDG_CONFIG_HOME` (when absolute) else `$HOME/.config`; macOS:
/// `$HOME/Library/Application Support`; Windows: `%APPDATA%`.
#[must_use]
pub fn config_dir(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let get = |k: &str| env(k).filter(|v| !v.is_empty());
    let base = if cfg!(target_os = "windows") {
        PathBuf::from(get("APPDATA")?)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(get("HOME")?).join("Library").join("Application Support")
    } else {
        match get("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()) {
            Some(xdg) => xdg,
            None => PathBuf::from(get("HOME")?).join(".config"),
        }
    };
    Some(base.join(APP_IDENTIFIER))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn linux_prefers_an_absolute_xdg_dir() {
        let env = |k: &str| match k {
            "XDG_CONFIG_HOME" => Some("/x/cfg".to_owned()),
            "HOME" => Some("/home/u".to_owned()),
            _ => None,
        };
        assert_eq!(config_dir(&env).unwrap(), PathBuf::from("/x/cfg").join(APP_IDENTIFIER));
        let relative = |k: &str| {
            if k == "XDG_CONFIG_HOME" {
                Some("rel".to_owned())
            } else if k == "HOME" {
                Some("/home/u".to_owned())
            } else {
                None
            }
        };
        assert_eq!(config_dir(&relative).unwrap(), PathBuf::from("/home/u/.config").join(APP_IDENTIFIER));
        assert_eq!(config_dir(&|_| None), None);
    }
}
