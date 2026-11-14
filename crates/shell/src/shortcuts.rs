//! Keyboard shortcuts come from the command registry, not from key handlers in components.
//! They are parsed here, so a bad shortcut string fails a Rust test, and the frontend receives
//! them as data and only compares fields with the key event.

use serde::Serialize;

/// A parsed shortcut. `cmd` is the platform's command key: Cmd on macOS, Ctrl elsewhere.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Shortcut {
    /// As written in the registry, e.g. `Cmd+Shift+Z`.
    pub text: String,
    pub cmd: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// The lowercased `KeyboardEvent.key` this shortcut matches, e.g. `z`, `enter`, `f2`.
    pub key: String,
}

const NAMED_KEYS: &[&str] =
    &["Enter", "Escape", "Tab", "Backspace", "Delete", "Insert", "Home", "End", "PageUp", "PageDown", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"];

/// The lowercased `KeyboardEvent.key` for a key name, or `None` for a name that is not a key.
fn key_value(name: &str) -> Option<String> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return (c.is_ascii_alphanumeric() || ",./;'[]\\-=`".contains(c)).then(|| c.to_ascii_lowercase().to_string());
    }
    if name == "Space" {
        return Some(" ".to_string());
    }
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()) {
        return (1..=12).contains(&n).then(|| name.to_ascii_lowercase());
    }
    NAMED_KEYS.contains(&name).then(|| name.to_ascii_lowercase())
}

/// `Cmd+Shift+Z` to a [`Shortcut`]: modifiers (`Cmd`, `Ctrl`, `Shift`, `Alt`) and exactly one key.
pub fn parse(shortcut: &str) -> Option<Shortcut> {
    let mut parsed = Shortcut { text: shortcut.to_string(), cmd: false, ctrl: false, shift: false, alt: false, key: String::new() };
    for part in shortcut.split('+') {
        match part {
            "Cmd" => parsed.cmd = true,
            "Ctrl" => parsed.ctrl = true,
            "Shift" => parsed.shift = true,
            "Alt" => parsed.alt = true,
            name => {
                if !parsed.key.is_empty() {
                    return None;
                }
                parsed.key = key_value(name)?;
            }
        }
    }
    (!parsed.key.is_empty()).then_some(parsed)
}

#[cfg(test)]
mod tests {
    use pistudiodashboard_engine::command_specs;

    use super::*;

    #[test]
    fn every_registered_shortcut_parses_and_is_unique_across_engine_and_shell() {
        let engine = command_specs().iter().map(|c| (c.id, c.shortcut));
        let shell = crate::menus::UI_COMMANDS.iter().map(|c| (c.id, c.shortcut));
        let mut seen = std::collections::HashSet::new();
        for (id, shortcut) in engine.chain(shell) {
            if let Some(s) = shortcut {
                assert!(parse(s).is_some(), "{id}: cannot parse shortcut `{s}`");
                assert!(seen.insert(s), "shortcut `{s}` is bound twice");
            }
        }
        assert!(seen.contains("Cmd+Shift+A"));
    }

    #[test]
    fn modifiers_and_one_key() {
        let z = parse("Cmd+Shift+Z").unwrap();
        assert_eq!((z.cmd, z.ctrl, z.shift, z.alt, z.key.as_str()), (true, false, true, false, "z"));
        assert_eq!(parse("Alt+F4").unwrap().key, "f4");
        assert_eq!(parse("Ctrl+Space").unwrap().key, " ");
        assert_eq!(parse("Delete").unwrap().key, "delete");
        for bad in ["Cmd+NoSuchKey", "Cmd+", "Cmd", "Cmd+A+B", "F13", ""] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }
}
