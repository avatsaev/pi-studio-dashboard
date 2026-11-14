//! Dependency layering rules.
//!
//! The rule engine works on a small model so it can be unit-tested; `from_metadata` builds that
//! model from `cargo metadata`. A new crate must be added to [`TABLE`] or the check fails.

use serde_json::Value;

/// The package-name prefix of this workspace's crates.
pub const PREFIX: &str = "pistudiodashboard-";

/// Where a workspace crate sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// A layered crate: it may depend only on crates in lower layers.
    Layer(u8),
    /// Layer 0 and no workspace dependency at all (format parsers, codecs: publishable alone).
    Standalone,
    /// Test tooling: other crates may use it only as a dev-dependency.
    Testkit,
    /// Binaries and build tooling: exempt from the rules.
    Exempt,
}

impl Class {
    fn layer(self) -> Option<u8> {
        match self {
            Class::Layer(l) => Some(l),
            Class::Standalone => Some(0),
            Class::Testkit => Some(UI_MIN_LAYER),
            Class::Exempt => None,
        }
    }
}

/// The layering table. Names are package names without [`PREFIX`].
///
/// ```text
/// L0  protocol            pi-studio wire types and the outbound allowlist (standalone)
/// L1  model               MonitorState, bounds, events and effects: pure data
/// L2  ingest              pure reducers and view derivations
/// L3  config              config file model, validation and storage
/// L4  client              tokio connections to daemons (the only layered crate with a runtime)
/// L5  engine              Session + command registry + effect executor
/// L6  shell               the shell (what the webview draws)
/// ```
pub const TABLE: &[(&str, Class)] = &[
    ("protocol", Class::Standalone),
    ("model", Class::Layer(1)),
    ("ingest", Class::Layer(2)),
    ("config", Class::Layer(3)),
    ("client", Class::Layer(CLIENT_LAYER)),
    ("engine", Class::Layer(5)),
    ("shell", Class::Layer(6)),
    ("testkit", Class::Testkit),
    // Apps and tooling.
    ("pistudiodashboard", Class::Exempt),
    ("cli", Class::Exempt),
    ("xtask", Class::Exempt),
];

/// The layer of the tokio-based `client` crate: the only layered crate that may use
/// [`FORBIDDEN_DEPS`] (app-structure.md, "Layer numbers").
pub const CLIENT_LAYER: u8 = 4;

/// Layered crates that must build for wasm32 (`cargo xtask wasm` checks exactly these).
/// Later tasks add and remove crates here; a `Layer`/`Standalone` crate in [`TABLE`] that is in
/// neither this list nor [`WASM_EXCLUDED`] fails `cargo xtask layers`.
pub const WASM_CRATES: &[&str] = &["protocol", "model", "ingest", "config", "engine", "shell"];

/// Layered crates deliberately not built for wasm32 (they own the tokio runtime).
pub const WASM_EXCLUDED: &[&str] = &["client"];

/// External crates (and the workspace `client`) that no layered crate except `client` may depend
/// on, dev-dependencies included. [`UI_CRATES`] (`tauri*`, ...) is forbidden on top of this list.
pub const FORBIDDEN_DEPS: &[&str] = &["pistudiodashboard-client", "tokio", "tokio-tungstenite", "keyring"];

/// Explicit orderings within a layer (earlier may be used by later), e.g. `&["render", "gpu"]`
/// when a GPU backend reuses the CPU reference.
pub const INTRA_LAYER_ORDER: &[&[&str]] = &[];

/// External crates that are a UI toolkit, a webview or a windowing dependency. A trailing `*`
/// is a prefix. wgpu is not listed: a renderer below the UI layer may use the GPU, but no
/// windows and no webviews.
pub const UI_CRATES: &[&str] = &["tauri*", "wry", "tao", "muda", "webview2*", "webkit2gtk*", "winit", "rfd"];

/// The first layer allowed to use [`UI_CRATES`].
pub const UI_MIN_LAYER: u8 = 6;

pub fn short_name(pkg: &str) -> &str {
    pkg.strip_prefix(PREFIX).unwrap_or(pkg)
}

pub fn classify(pkg: &str) -> Option<Class> {
    let short = short_name(pkg);
    TABLE.iter().find(|(name, _)| *name == short).map(|(_, class)| *class)
}

fn intra_layer_allowed(from: &str, to: &str) -> bool {
    let (from, to) = (short_name(from), short_name(to));
    INTRA_LAYER_ORDER.iter().any(|chain| match (chain.iter().position(|n| *n == from), chain.iter().position(|n| *n == to)) {
        (Some(f), Some(t)) => t < f,
        _ => false,
    })
}

fn is_ui_crate(name: &str) -> bool {
    UI_CRATES.iter().any(|p| match p.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == *p,
    })
}

/// A dependency no layered crate except `client` may have: [`FORBIDDEN_DEPS`] or a UI crate.
fn is_forbidden(name: &str) -> bool {
    FORBIDDEN_DEPS.contains(&name) || is_ui_crate(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

#[derive(Debug, Clone)]
pub struct Dep {
    pub name: String,
    pub kind: DepKind,
    /// `true` if the dependency is a workspace member.
    pub workspace: bool,
}

#[derive(Debug, Clone)]
pub struct Crate {
    pub name: String,
    pub deps: Vec<Dep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    Unregistered { krate: String },
    Upward { krate: String, dep: String, from: u8, to: u8 },
    StandaloneHasWorkspaceDep { krate: String, dep: String },
    TestkitAsNormalDep { krate: String },
    UiBelowUiLayer { krate: String, dep: String, layer: u8 },
    ForbiddenDep { krate: String, dep: String },
    WasmListing { krate: String, listed: usize },
    WasmUnknown { krate: String },
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::Unregistered { krate } => write!(f, "{krate}: not in the layering table (xtask/src/layers.rs TABLE)"),
            Violation::Upward { krate, dep, from, to } => write!(f, "{krate} (L{from}) depends on {dep} (L{to}): a crate may depend only on lower layers"),
            Violation::StandaloneHasWorkspaceDep { krate, dep } => write!(f, "{krate} is standalone but depends on workspace crate {dep}"),
            Violation::TestkitAsNormalDep { krate } => write!(f, "{krate} uses the testkit outside [dev-dependencies]"),
            Violation::UiBelowUiLayer { krate, dep, layer } => write!(f, "{krate} (L{layer}) depends on UI crate {dep}: only L{UI_MIN_LAYER} and above may"),
            Violation::ForbiddenDep { krate, dep } => write!(
                f,
                "{krate} depends on {dep}: forbidden below the apps (FORBIDDEN_DEPS, app-structure.md layer map); only `client` (L{CLIENT_LAYER}) and the apps may use it"
            ),
            Violation::WasmListing { krate, listed } => {
                write!(f, "{krate}: must be in exactly one of WASM_CRATES / WASM_EXCLUDED (xtask/src/layers.rs), found in {listed}")
            }
            Violation::WasmUnknown { krate } => write!(f, "{krate}: named in WASM_CRATES / WASM_EXCLUDED but not a layered crate in TABLE"),
        }
    }
}

/// Check all rules. Returns violations sorted for stable output.
pub fn check(crates: &[Crate]) -> Vec<Violation> {
    let mut out = Vec::new();
    for c in crates {
        let Some(class) = classify(&c.name) else {
            out.push(Violation::Unregistered { krate: c.name.clone() });
            continue;
        };
        let Some(layer) = class.layer() else { continue };
        // Everything layered except `client` (L4) must stay free of sockets, runtimes and UI.
        let forbidden_scope = layer != CLIENT_LAYER && class != Class::Testkit;
        for d in &c.deps {
            // A self dev-dependency (to enable features in tests) is fine.
            if d.name == c.name {
                continue;
            }
            if !d.workspace {
                if forbidden_scope && is_forbidden(&d.name) {
                    out.push(Violation::ForbiddenDep { krate: c.name.clone(), dep: d.name.clone() });
                } else if layer < UI_MIN_LAYER && is_ui_crate(&d.name) {
                    out.push(Violation::UiBelowUiLayer { krate: c.name.clone(), dep: d.name.clone(), layer });
                }
                continue;
            }
            if forbidden_scope && is_forbidden(&d.name) {
                out.push(Violation::ForbiddenDep { krate: c.name.clone(), dep: d.name.clone() });
                continue;
            }
            if class == Class::Standalone {
                out.push(Violation::StandaloneHasWorkspaceDep { krate: c.name.clone(), dep: d.name.clone() });
                continue;
            }
            match classify(&d.name) {
                // An unregistered dependency is reported on its own entry.
                None => {}
                Some(Class::Testkit) if d.kind != DepKind::Dev => out.push(Violation::TestkitAsNormalDep { krate: c.name.clone() }),
                Some(Class::Testkit) => {}
                Some(dep_class) => {
                    // Apps (exempt) sit above every layer: nothing layered may depend on one.
                    let to = dep_class.layer().unwrap_or(UI_MIN_LAYER + 1);
                    if to >= layer && !(to == layer && intra_layer_allowed(&c.name, &d.name)) {
                        out.push(Violation::Upward { krate: c.name.clone(), dep: d.name.clone(), from: layer, to });
                    }
                }
            }
        }
    }
    out.sort_by_key(|v| v.to_string());
    out.dedup();
    out
}

/// Wasm-list consistency: every `Layer`/`Standalone` crate in [`TABLE`] that exists in the
/// workspace is in exactly one of `wasm` and `excluded`; every list entry is in [`TABLE`] as a
/// layered crate. Takes the lists as arguments so tests can use synthetic ones.
pub fn check_wasm_lists(crates: &[Crate], wasm: &[&str], excluded: &[&str]) -> Vec<Violation> {
    let mut out = Vec::new();
    for c in crates {
        if !matches!(classify(&c.name), Some(Class::Layer(_) | Class::Standalone)) {
            continue;
        }
        let short = short_name(&c.name);
        let n = wasm.iter().filter(|n| **n == short).count() + excluded.iter().filter(|n| **n == short).count();
        if n != 1 {
            out.push(Violation::WasmListing { krate: short.to_owned(), listed: n });
        }
    }
    for name in wasm.iter().chain(excluded) {
        if !matches!(TABLE.iter().find(|(t, _)| t == name), Some((_, Class::Layer(_) | Class::Standalone))) {
            out.push(Violation::WasmUnknown { krate: (*name).to_owned() });
        }
    }
    out.sort_by_key(|v| v.to_string());
    out.dedup();
    out
}

/// Build the model from `cargo metadata --format-version 1 --no-deps`.
pub fn from_metadata(meta: &Value) -> Result<Vec<Crate>, String> {
    let pkgs = meta["packages"].as_array().ok_or("metadata: no packages array")?;
    let members: Vec<&str> = pkgs.iter().filter_map(|p| p["name"].as_str()).collect();
    let mut out = Vec::new();
    for p in pkgs {
        let name = p["name"].as_str().ok_or("package without name")?.to_owned();
        let mut deps = Vec::new();
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let dep_name = d["name"].as_str().unwrap_or_default().to_owned();
            let kind = match d["kind"].as_str() {
                Some("dev") => DepKind::Dev,
                Some("build") => DepKind::Build,
                _ => DepKind::Normal,
            };
            let workspace = members.contains(&dep_name.as_str()) || d["path"].is_string();
            deps.push(Dep { name: dep_name, kind, workspace });
        }
        out.push(Crate { name, deps });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn describe(class: Option<Class>) -> String {
    match class {
        Some(Class::Layer(l)) => format!("L{l}"),
        Some(Class::Standalone) => "L0 standalone".into(),
        Some(Class::Testkit) => "testkit".into(),
        Some(Class::Exempt) => "app/tooling".into(),
        None => "UNREGISTERED".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn krate(name: &str, deps: &[(&str, DepKind, bool)]) -> Crate {
        Crate { name: name.into(), deps: deps.iter().map(|(n, kind, workspace)| Dep { name: (*n).into(), kind: *kind, workspace: *workspace }).collect() }
    }

    #[test]
    fn downward_dependencies_pass() {
        let crates = [
            krate(
                "pistudiodashboard-engine",
                &[
                    ("pistudiodashboard-model", DepKind::Normal, true),
                    ("pistudiodashboard-testkit", DepKind::Dev, true),
                    ("serde_json", DepKind::Normal, false),
                ],
            ),
            krate("pistudiodashboard-shell", &[("pistudiodashboard-engine", DepKind::Normal, true), ("serde", DepKind::Normal, false)]),
            krate("pistudiodashboard", &[("pistudiodashboard-shell", DepKind::Normal, true), ("tauri", DepKind::Normal, false)]),
        ];
        assert_eq!(check(&crates), []);
    }

    #[test]
    fn each_rule_reports_its_violation() {
        let crates = [
            krate("pistudiodashboard-model", &[("pistudiodashboard-engine", DepKind::Normal, true)]),
            krate(
                "pistudiodashboard-ingest",
                &[("pistudiodashboard-model", DepKind::Normal, true), ("pistudiodashboard-ingest", DepKind::Dev, true), ("tauri", DepKind::Normal, false)],
            ),
            krate("pistudiodashboard-config", &[("pistudiodashboard-testkit", DepKind::Normal, true), ("tauri-plugin-dialog", DepKind::Dev, false)]),
            krate("pistudiodashboard-engine", &[("pistudiodashboard", DepKind::Normal, true)]),
            krate("pistudiodashboard-client", &[("tauri-plugin-dialog", DepKind::Normal, false)]),
            krate("pistudiodashboard-new", &[]),
        ];
        let found: Vec<String> = check(&crates).iter().map(ToString::to_string).collect();
        assert_eq!(found.len(), 7, "{found:#?}");
        for expected in [
            "pistudiodashboard-model (L1) depends on pistudiodashboard-engine (L5)",
            "pistudiodashboard-ingest depends on tauri: forbidden",
            "pistudiodashboard-config depends on tauri-plugin-dialog: forbidden",
            "pistudiodashboard-config uses the testkit outside [dev-dependencies]",
            "pistudiodashboard-engine (L5) depends on pistudiodashboard (L7)",
            "pistudiodashboard-client (L4) depends on UI crate tauri-plugin-dialog",
            "pistudiodashboard-new: not in the layering table",
        ] {
            assert!(found.iter().any(|f| f.starts_with(expected)), "missing `{expected}` in {found:#?}");
        }
    }

    const FORBIDDEN_SAMPLES: [&str; 5] = ["pistudiodashboard-client", "tokio", "tokio-tungstenite", "keyring", "tauri-plugin-notification"];

    fn forbidden(krate_name: &str, dep: &str) -> Violation {
        Violation::ForbiddenDep { krate: krate_name.into(), dep: dep.into() }
    }

    #[test]
    fn engine_shell_and_core_layers_reject_forbidden_deps() {
        for name in ["engine", "shell", "protocol", "model", "ingest", "config"] {
            let full = format!("pistudiodashboard-{name}");
            for dep in FORBIDDEN_SAMPLES {
                let workspace = dep == "pistudiodashboard-client";
                let found = check(&[krate(&full, &[(dep, DepKind::Normal, workspace)])]);
                assert!(found.contains(&forbidden(&full, dep)), "{full} -> {dep}: {found:?}");
                // Dev-dependencies are covered too.
                let found = check(&[krate(&full, &[(dep, DepKind::Dev, workspace)])]);
                assert!(found.contains(&forbidden(&full, dep)), "{full} -(dev)-> {dep}: {found:?}");
            }
        }
    }

    #[test]
    fn client_may_use_runtime_crates_but_not_the_engine() {
        let ok = krate(
            "pistudiodashboard-client",
            &[
                ("tokio", DepKind::Normal, false),
                ("tokio-tungstenite", DepKind::Normal, false),
                ("pistudiodashboard-protocol", DepKind::Normal, true),
                ("pistudiodashboard-model", DepKind::Normal, true),
                ("pistudiodashboard-ingest", DepKind::Normal, true),
                ("pistudiodashboard-config", DepKind::Normal, true),
                ("pistudiodashboard-testkit", DepKind::Dev, true),
            ],
        );
        assert_eq!(check(&[ok]), []);
        let up = check(&[krate("pistudiodashboard-client", &[("pistudiodashboard-engine", DepKind::Normal, true)])]);
        assert_eq!(up, [Violation::Upward { krate: "pistudiodashboard-client".into(), dep: "pistudiodashboard-engine".into(), from: 4, to: 5 }]);
    }

    #[test]
    fn testkit_may_use_tokio() {
        assert_eq!(check(&[krate("pistudiodashboard-testkit", &[("tokio", DepKind::Normal, false)])]), []);
    }

    #[test]
    fn engine_to_client_is_a_forbidden_dep_not_just_upward() {
        let found = check(&[krate("pistudiodashboard-engine", &[("pistudiodashboard-client", DepKind::Normal, true)])]);
        assert_eq!(found, [forbidden("pistudiodashboard-engine", "pistudiodashboard-client")]);
        assert!(found[0].to_string().contains("FORBIDDEN_DEPS"));
    }

    #[test]
    fn wasm_lists_cover_every_layered_crate_exactly_once() {
        let crates = [krate("pistudiodashboard-protocol", &[]), krate("pistudiodashboard-client", &[]), krate("pistudiodashboard-cli", &[])];
        assert_eq!(check_wasm_lists(&crates, &["protocol"], &["client"]), []);
        let missing = check_wasm_lists(&crates, &["protocol"], &[]);
        assert_eq!(missing, [Violation::WasmListing { krate: "client".into(), listed: 0 }]);
        assert!(missing[0].to_string().starts_with("client:"));
        let twice = check_wasm_lists(&crates, &["protocol", "client"], &["client"]);
        assert_eq!(twice, [Violation::WasmListing { krate: "client".into(), listed: 2 }]);
        let unknown = check_wasm_lists(&crates, &["protocol", "cli"], &["client"]);
        assert_eq!(unknown, [Violation::WasmUnknown { krate: "cli".into() }]);
    }

    #[test]
    fn real_wasm_lists_are_consistent_with_the_table() {
        let all: Vec<Crate> = TABLE.iter().map(|(n, _)| krate(&format!("{PREFIX}{n}"), &[])).collect();
        assert_eq!(check_wasm_lists(&all, WASM_CRATES, WASM_EXCLUDED), []);
    }
}
