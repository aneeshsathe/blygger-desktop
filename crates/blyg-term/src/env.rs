//! The environment the pane's shell gets.
//!
//! The shell is the user's own login shell with the user's own rights, as
//! Terminal.app's is, so it keeps the user's environment (`PATH`, `HOME`,
//! `SSH_AUTH_SOCK`, their locale…). What it never gets is anything of
//! Burrow's: every `BLYGGER_*` variable (one of them, in tests, is a token),
//! the variables Burrow sets for itself (`GPUI_*`), and the identity of
//! whatever terminal launched Burrow (so programs don't think they're in
//! iTerm or tmux). Burrow never adds a token, a path to its data, or its
//! database to it.

use std::ffi::{OsStr, OsString};

/// Prefixes of variables the shell never sees (compared ignoring case).
pub const DROP_PREFIXES: &[&str] = &[
    // Burrow's own: its settings, test hooks and the test token.
    "BLYGGER_",
    // What Burrow sets for itself (GPUI_DISABLE_DIRECT_COMPOSITION).
    "GPUI_",
    // The terminal Burrow was launched from, if any.
    "TERM_PROGRAM",
    "TERM_SESSION_ID",
    "ITERM_",
    "GHOSTTY_",
    "KITTY_",
    "WEZTERM_",
    "ALACRITTY_",
    "VSCODE_",
    "ZED_",
    "WT_SESSION",
    "WT_PROFILE_ID",
    "TMUX",
];

/// Variables dropped by exact name.
pub const DROP_EXACT: &[&str] = &["COLUMNS", "LINES", "TERM", "COLORTERM", "TERMINFO", "STY"];

/// What the shell is told about its terminal.
pub const SET: &[(&str, &str)] = &[
    ("TERM", "xterm-256color"),
    ("COLORTERM", "truecolor"),
    ("TERM_PROGRAM", "Burrow"),
];

/// Whether the shell never sees `name`.
pub fn dropped(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        // A name that isn't Unicode is nothing of ours; keep it.
        return false;
    };
    let upper = name.to_ascii_uppercase();
    DROP_PREFIXES.iter().any(|p| upper.starts_with(p)) || DROP_EXACT.contains(&upper.as_str())
}

/// The shell's environment from Burrow's own (`vars`, e.g.
/// `std::env::vars_os()`): every variable but the dropped ones, plus
/// [`SET`].
pub fn scrub(vars: impl IntoIterator<Item = (OsString, OsString)>) -> Vec<(OsString, OsString)> {
    let mut out: Vec<(OsString, OsString)> =
        vars.into_iter().filter(|(k, _)| !dropped(k)).collect();
    for (k, v) in SET {
        out.push((k.into(), v.into()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(env: &[(OsString, OsString)]) -> Vec<String> {
        env.iter()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn burrows_variables_never_reach_the_shell() {
        let parent = [
            ("PATH", "/usr/bin:/bin"),
            ("HOME", "/Users/ada"),
            ("SSH_AUTH_SOCK", "/tmp/agent.sock"),
            ("BLYGGER_TEST_TOKEN", "secret-token"),
            ("BLYGGER_DATA_DIR", "/data"),
            ("blygger_fake", "1"),
            ("GPUI_DISABLE_DIRECT_COMPOSITION", "1"),
            ("TERM_PROGRAM", "iTerm.app"),
            ("TERM_PROGRAM_VERSION", "3"),
            ("ITERM_SESSION_ID", "x"),
            ("TMUX", "/tmp/tmux"),
            ("TERM", "dumb"),
            ("COLUMNS", "80"),
        ]
        .map(|(k, v)| (OsString::from(k), OsString::from(v)));
        let env = scrub(parent);
        let n = names(&env);
        assert!(
            n.iter()
                .all(|k| !k.to_ascii_uppercase().starts_with("BLYGGER_"))
        );
        assert!(!env.iter().any(|(_, v)| v == "secret-token"));
        assert!(!n.contains(&"GPUI_DISABLE_DIRECT_COMPOSITION".into()));
        assert!(!n.contains(&"ITERM_SESSION_ID".into()));
        assert!(!n.contains(&"TMUX".into()));
        assert!(!n.contains(&"COLUMNS".into()));
        for keep in ["PATH", "HOME", "SSH_AUTH_SOCK"] {
            assert!(n.contains(&keep.into()), "{keep}");
        }
        let get = |k: &str| {
            env.iter()
                .filter(|(n, _)| n == k)
                .map(|(_, v)| v.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(get("TERM"), ["xterm-256color"]);
        assert_eq!(get("TERM_PROGRAM"), ["Burrow"]);
        assert_eq!(get("TERM_PROGRAM_VERSION"), Vec::<String>::new());
    }
}
