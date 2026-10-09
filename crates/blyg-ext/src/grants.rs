//! What the user allowed, per extension: the config file's
//! `extension-allow = <name> <capability>` lines, and the extension's
//! `extension-setting = <name> key=value` lines. The config layer reads the
//! file; these take the values as plain strings so the host doesn't depend
//! on it.

use std::collections::BTreeMap;

use crate::capability::Capability;
use crate::manifest::valid_name;

/// Granted capabilities, by extension name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grants {
    by_ext: BTreeMap<String, Vec<Capability>>,
}

impl Grants {
    /// From `extension-allow` values (`markdown-notes fs:~/Notes`). Lines
    /// that don't parse are returned as messages and skipped.
    pub fn from_allow_lines<S: AsRef<str>>(lines: &[S]) -> (Grants, Vec<String>) {
        let mut g = Grants::default();
        let mut bad = vec![];
        for line in lines {
            let line = line.as_ref().trim();
            let Some((name, cap)) = line.split_once(char::is_whitespace) else {
                bad.push(format!(
                    "extension-allow = {line}: expected `<name> <capability>`"
                ));
                continue;
            };
            if !valid_name(name) {
                bad.push(format!(
                    "extension-allow = {line}: {name:?} isn't an extension name"
                ));
                continue;
            }
            match Capability::parse(cap) {
                Some(c) => g.grant(name, c),
                None => bad.push(format!(
                    "extension-allow = {line}: unknown capability {:?}",
                    cap.trim()
                )),
            }
        }
        (g, bad)
    }

    pub fn grant(&mut self, name: &str, cap: Capability) {
        let v = self.by_ext.entry(name.to_string()).or_default();
        if !v.iter().any(|c| cap.covered_by(c)) {
            v.push(cap);
        }
    }

    /// Everything granted to `name`.
    pub fn of(&self, name: &str) -> &[Capability] {
        self.by_ext.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Whether `name` holds `cap`.
    pub fn allows(&self, name: &str, cap: &Capability) -> bool {
        self.of(name).iter().any(|g| cap.covered_by(g))
    }

    /// Whether the user has granted `name` anything at all (has answered
    /// its consent sheet at least once).
    pub fn has_any(&self, name: &str) -> bool {
        !self.of(name).is_empty()
    }

    /// The capabilities of `requested` that `name` hasn't been granted.
    pub fn missing(&self, name: &str, requested: &[Capability]) -> Vec<Capability> {
        requested
            .iter()
            .filter(|c| !self.allows(name, c))
            .cloned()
            .collect()
    }

    /// The `extension-allow` values for `name` granting `caps`, for the
    /// consent sheet to write back.
    pub fn allow_lines(name: &str, caps: &[Capability]) -> Vec<String> {
        caps.iter().map(|c| format!("{name} {c}")).collect()
    }
}

/// From `extension-setting` values (`markdown-notes vault=~/Notes`):
/// `name -> key -> value`. Later lines win. Bad lines come back as messages.
pub fn settings_from_lines<S: AsRef<str>>(
    lines: &[S],
) -> (BTreeMap<String, BTreeMap<String, String>>, Vec<String>) {
    let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut bad = vec![];
    for line in lines {
        let line = line.as_ref().trim();
        let parsed = line.split_once(char::is_whitespace).and_then(|(name, kv)| {
            kv.trim()
                .split_once('=')
                .map(|(k, v)| (name, k.trim(), v.trim()))
        });
        match parsed {
            Some((name, k, v)) if valid_name(name) && !k.is_empty() => {
                out.entry(name.into())
                    .or_default()
                    .insert(k.into(), v.into());
            }
            _ => bad.push(format!(
                "extension-setting = {line}: expected `<name> key=value`"
            )),
        }
    }
    (out, bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_allow_lines() {
        let (g, bad) = Grants::from_allow_lines(&[
            "markdown-notes ui",
            "markdown-notes   fs:~/Notes",
            "markdown-notes fs:~\\Notes\\",
            "other items.read",
            "Bad-Name ui",
            "markdown-notes items.publish",
            "lonely",
        ]);
        assert_eq!(bad.len(), 3, "{bad:?}");
        assert_eq!(
            g.of("markdown-notes").len(),
            2,
            "the two fs: spellings are one grant"
        );
        assert!(g.allows("markdown-notes", &Capability::Ui));
        assert!(!g.allows("markdown-notes", &Capability::ItemsRead));
        assert!(g.allows("other", &Capability::ItemsRead));
        assert!(!g.has_any("nobody"));
        assert_eq!(
            g.missing("markdown-notes", &[Capability::Ui, Capability::Net]),
            vec![Capability::Net]
        );
        assert_eq!(
            Grants::allow_lines("x", &[Capability::Ui, Capability::Fs("~/N".into())]),
            ["x ui", "x fs:~/N"]
        );
    }

    #[test]
    fn parses_setting_lines() {
        let (s, bad) = settings_from_lines(&[
            "markdown-notes vault=~/Notes",
            "markdown-notes vault = C:\\Users\\someone\\Notes",
            "markdown-notes folder=Inbox",
            "markdown-notes novalue",
        ]);
        assert_eq!(bad.len(), 1);
        assert_eq!(s["markdown-notes"]["vault"], "C:\\Users\\someone\\Notes");
        assert_eq!(s["markdown-notes"]["folder"], "Inbox");
    }
}
