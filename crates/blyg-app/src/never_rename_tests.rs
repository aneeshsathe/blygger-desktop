//! The names existing installs depend on, pinned. The app was renamed
//! Blygger → Burrow in display only (`docs/SPEC.md`, "Naming"): renaming any
//! of these strands a user's data, Keychain items, config or key bindings,
//! and nothing else would notice. Each value here is the one that shipped;
//! a change to one is a migration, not an edit to this test.

use std::ffi::OsString;
use std::path::PathBuf;

/// A fake environment: `HOME` plus Windows' app folders.
fn env(k: &str) -> Option<OsString> {
    match k {
        "HOME" | "USERPROFILE" => Some("/h".into()),
        "APPDATA" => Some("/appdata".into()),
        "LOCALAPPDATA" => Some("/localappdata".into()),
        _ => None,
    }
}

#[test]
fn bundle_id_and_keychain_service() {
    assert_eq!(blyg_core::config::APP_ID, "org.blygger.desktop");
    assert_eq!(crate::update::install::BUNDLE_ID, "org.blygger.desktop");
    assert_eq!(blyg_core::config::KEYCHAIN_SERVICE, "org.blygger.desktop");
    // The packaging scripts read these (Info.plist's CFBundleIdentifier and
    // CFBundleExecutable).
    let packaging = include_str!("../../../packaging/config.env");
    for line in ["BUNDLE_ID=org.blygger.desktop", "BINARY_NAME=blygger"] {
        assert!(
            packaging.lines().any(|l| l.trim() == line),
            "packaging/config.env lost `{line}`"
        );
    }
}

#[test]
fn keychain_accounts() {
    use blyg_ai::{accounts, chatgpt, cloudflare};
    assert_eq!(
        accounts::ANTHROPIC_KEY_ACCOUNT,
        "blygger-ai.anthropic-api-key"
    );
    assert_eq!(accounts::OPENAI_KEY_ACCOUNT, "blygger-ai.openai-api-key");
    assert_eq!(chatgpt::KEYCHAIN_ACCOUNT, "blygger-ai.chatgpt-oauth");
    assert_eq!(
        cloudflare::KEYCHAIN_ACCOUNT,
        "blygger-ai.cloudflare-api-token"
    );
    // A blyg's token is kept under its host.
    assert_eq!(
        blyg_core::config::token_account("https://blyg.example.com/"),
        "blyg.example.com"
    );
}

#[test]
fn data_dir_and_database() {
    use blyg_core::config::paths;
    let want = if cfg!(windows) {
        PathBuf::from("/localappdata").join("Blygger")
    } else {
        PathBuf::from("/h/Library/Application Support/org.blygger.desktop")
    };
    assert_eq!(paths::data_dir_with(&env), want);
    assert_eq!(paths::LEGACY_DATA_DIR_NAME, "Blygger");
    assert_eq!(blyg_core::live::DB_FILE, "blygger.db");
    assert_eq!(
        blyg_core::config::blygs::DB_OWNER_FILE,
        "blygger.db.blyg-url"
    );
    // The browser pane's WebKit data store (its cookies and logins).
    assert_eq!(
        &crate::app::browser::surface::DATA_STORE_ID,
        b"blygger-browser\0"
    );
}

#[test]
fn config_path() {
    use blyg_core::config::paths;
    let load = paths::default_locations(&env);
    assert_eq!(load[0], PathBuf::from("/h/.config/blygger/config"));
    let second = if cfg!(windows) {
        PathBuf::from("/appdata").join("Blygger").join("config")
    } else {
        PathBuf::from("/h/Library/Application Support/org.blygger.desktop/config")
    };
    assert_eq!(load[1], second);
}

#[test]
fn binary_name() {
    assert_eq!(env!("CARGO_BIN_NAME"), "blygger");
}

#[test]
fn key_contexts() {
    assert_eq!(crate::app::CONTEXT, "Blygger");
    assert_eq!(crate::keymap::MAIN_INPUT, "Blygger > Input");
    assert_eq!(crate::keymap::BROWSER, "Blygger > Browser");
    assert_eq!(crate::keymap::MACRO_SHEET, "MacroSheet");
    assert_eq!(crate::keymap::MACRO_SHEET_INPUT, "MacroSheet > Input");
    assert_eq!(crate::app::notes::CONTEXT, "Notes");
}

/// Every action a key is bound to keeps its namespace (user keybinding
/// configs name actions as `blygger::NewDraft`).
#[test]
fn action_namespaces() {
    const NAMESPACES: &[&str] = &[
        "blygger",
        "blygger_ai",
        "blygger_nearby",
        "blygger_onboarding",
    ];
    let mut bad = Vec::new();
    for row in crate::keymap::mac_table() {
        let Some(make) = row.new_action else { continue };
        let name = make().name();
        let ns = name.split("::").next().unwrap_or_default();
        if !NAMESPACES.contains(&ns) {
            bad.push(name);
        }
    }
    assert!(
        bad.is_empty(),
        "actions outside the shipped namespaces: {bad:?}"
    );
    let new_draft = crate::keymap::mac_table()
        .into_iter()
        .find(|r| r.action == "NewDraft")
        .and_then(|r| r.new_action)
        .expect("NewDraft is bound");
    assert_eq!(new_draft().name(), "blygger::NewDraft");
}

/// Every variable dev_flags.rs reads (DevFlags and the tests' TestEnv), as
/// the string literals it names outside its own tests. Scripts, CI and
/// people's shells set these; a renamed one is silently ignored.
#[test]
fn env_var_names() {
    const NAMES: &[&str] = &[
        "BLYGGER_BROWSER_EPHEMERAL",
        "BLYGGER_BROWSER_TEARDOWN_MS",
        "BLYGGER_DATA_DIR",
        "BLYGGER_DEMO",
        "BLYGGER_DEMO_FOLD",
        "BLYGGER_DEMO_FROM",
        "BLYGGER_DEMO_HOLD_MS",
        "BLYGGER_DEMO_HOVER",
        "BLYGGER_DEMO_NOTE",
        "BLYGGER_DEMO_PREVIEW",
        "BLYGGER_DEMO_SCROLL",
        "BLYGGER_DEMO_SITE_URL",
        "BLYGGER_DEMO_SLOT",
        "BLYGGER_DEMO_URL",
        "BLYGGER_DEMO_URL2",
        "BLYGGER_FAKE",
        "BLYGGER_FAKE_NO_PROVENANCE",
        "BLYGGER_FAKE_NO_READ",
        "BLYGGER_FAKE_STUDIO_LATEST",
        "BLYGGER_LISTS_DIR",
        "BLYGGER_MACRO_TRACE",
        "BLYGGER_NO_ACTIVATE",
        "BLYGGER_NO_BLOCKLIST_DOWNLOAD",
        "BLYGGER_NO_UPDATE",
        "BLYGGER_SNAPSHOT",
        "BLYGGER_SNAPSHOT_DIR",
        "BLYGGER_SNAPSHOT_HEIGHT",
        "BLYGGER_SNAPSHOT_WIDTH",
        "BLYGGER_TEST_SPELLING",
        "BLYGGER_TEST_TOKEN",
        "BLYGGER_THEME",
        "BLYGGER_TIMING",
        "BLYGGER_UPDATE_DOCS",
        "BLYGGER_UPDATE_PUBKEY",
        "BLYGGER_UPDATE_SMOKE",
        "BLYGGER_UPDATE_URL",
        "BLYGGER_VERSION_AS",
        "BLYG_E2E_SWITCH_TOKEN",
        "BLYG_E2E_SWITCH_URL",
        "BLYG_E2E_TOKEN",
        "BLYG_E2E_URL",
        "BLYG_E2E_URL_B",
    ];
    let src = include_str!("dev_flags.rs");
    let src = src.split("\n#[cfg(test)]\nmod tests {").next().unwrap();
    let mut read: Vec<&str> = src
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| {
            s.starts_with("BLYG")
                && s.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        })
        .collect();
    read.sort_unstable();
    read.dedup();
    let gone: Vec<&&str> = NAMES.iter().filter(|n| !read.contains(n)).collect();
    let new: Vec<&&str> = read.iter().filter(|n| !NAMES.contains(n)).collect();
    assert!(
        gone.is_empty() && new.is_empty(),
        "the environment variables dev_flags.rs reads changed (a name is never \
         renamed: docs/SPEC.md, Naming; a new one is added here):\n  no longer read: {gone:?}\n  not pinned: {new:?}"
    );
}
