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
