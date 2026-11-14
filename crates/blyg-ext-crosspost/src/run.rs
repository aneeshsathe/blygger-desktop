//! The BXP extension around [`crate::text`]: it answers
//! `extension/macro.prepare` for its one macro and nothing else. Burrow
//! runs the recipe; this side never touches the browser, the network or
//! a file.

use std::collections::BTreeMap;
use std::process::ExitCode;

use blyg_ext::ext::{Extension, HostClient, serve};
use blyg_ext::protocol::*;
use blyg_ext::rpc::{params, to_value};
use serde_json::Value;

use crate::text::{
    DEFAULT_BLUESKY_MAX_CHARS, DEFAULT_FARCASTER_MAX_BYTES, Limit, limit_setting,
    max_chars_setting, prepare_within, template_setting,
};
use crate::{BLUESKY_MACRO, FARCASTER_MACRO, MACRO, NAME};

/// The extension's state: its settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossPostExt {
    template: String,
    max_chars: usize,
    bluesky_max_chars: usize,
    farcaster_max_bytes: usize,
    /// Settings it couldn't use (the default stands), for the preview
    /// sheet's note.
    problems: Vec<String>,
}

impl Default for CrossPostExt {
    fn default() -> Self {
        let mut e = CrossPostExt {
            template: String::new(),
            max_chars: 0,
            bluesky_max_chars: 0,
            farcaster_max_bytes: 0,
            problems: vec![],
        };
        e.configure(&BTreeMap::new());
        e
    }
}

impl CrossPostExt {
    /// Read `template`, `max-chars`, `bluesky-max-chars` and
    /// `farcaster-max-bytes` (`extension-setting = cross-post …`).
    pub fn configure(&mut self, settings: &BTreeMap<String, String>) {
        self.problems.clear();
        self.template = template_setting(settings.get("template").map(String::as_str))
            .unwrap_or_else(|(default, why)| {
                self.problems.push(why);
                default
            });
        self.max_chars = max_chars_setting(settings.get("max-chars").map(String::as_str))
            .unwrap_or_else(|(default, why)| {
                self.problems.push(why);
                default
            });
        let mut limit = |key: &str, default: usize| {
            limit_setting(key, settings.get(key).map(String::as_str), default).unwrap_or_else(
                |(default, why)| {
                    self.problems.push(why);
                    default
                },
            )
        };
        self.bluesky_max_chars = limit("bluesky-max-chars", DEFAULT_BLUESKY_MAX_CHARS);
        self.farcaster_max_bytes = limit("farcaster-max-bytes", DEFAULT_FARCASTER_MAX_BYTES);
    }

    /// The limit for one of its macros; `None` for any other.
    fn limit(&self, macro_id: &str) -> Option<Limit> {
        Some(match macro_id {
            MACRO => Limit::chars(self.max_chars, "max-chars"),
            BLUESKY_MACRO => Limit::chars(self.bluesky_max_chars, "bluesky-max-chars"),
            FARCASTER_MACRO => Limit::bytes(self.farcaster_max_bytes, "farcaster-max-bytes"),
            _ => return None,
        })
    }

    /// `extension/macro.prepare`.
    pub fn prepare(&self, p: &MacroPrepareParams) -> Result<MacroPrepareResult, RpcError> {
        let Some(limit) = self.limit(&p.macro_id) else {
            return Err(RpcError::invalid_params(format!(
                "{NAME} has no macro {:?}",
                p.macro_id
            )));
        };
        let out = prepare_within(
            p.item.content_md.as_deref(),
            &p.item.title,
            &p.permalink,
            &self.template,
            limit,
            &p.text,
        );
        let mut notes: Vec<String> = out.note.into_iter().collect();
        notes.extend(
            self.problems
                .iter()
                .map(|w| format!("{w}; using the default")),
        );
        Ok(MacroPrepareResult {
            text: out.text,
            note: (!notes.is_empty()).then(|| notes.join(" · ")),
        })
    }
}

impl Extension for CrossPostExt {
    fn initialize(
        &mut self,
        _host: &HostClient,
        p: InitializeParams,
    ) -> Result<InitializeResult, RpcError> {
        self.configure(&p.settings);
        Ok(InitializeResult {
            protocol_version: PROTOCOL_VERSION,
            name: NAME.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            commands: vec![],
            sources: vec![],
            libraries: vec![],
        })
    }

    fn request(&mut self, _host: &HostClient, method: &str, p: Value) -> Result<Value, RpcError> {
        match method {
            methods::MACRO_PREPARE => {
                let p: MacroPrepareParams = params(p)?;
                to_value(&self.prepare(&p)?)
            }
            m => Err(RpcError::method_not_found(m)),
        }
    }

    fn notification(&mut self, _host: &HostClient, method: &str, p: Value) {
        if method == methods::SETTINGS_CHANGED
            && let Ok(p) = serde_json::from_value::<SettingsChanged>(p)
        {
            self.configure(&p.settings);
        }
    }
}

/// Serve the extension over `read`/`write` until shutdown.
pub fn run<R, W>(read: R, write: W) -> ExitCode
where
    R: std::io::Read + Send + 'static,
    W: std::io::Write + Send + 'static,
{
    serve(read, write, &mut CrossPostExt::default());
    ExitCode::SUCCESS
}

/// [`run`] over this process's stdin/stdout (`blygger +ext cross-post`).
pub fn run_stdio() -> ExitCode {
    run(std::io::stdin(), std::io::stdout())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(md: Option<&str>) -> ItemSummary {
        ItemSummary {
            id: "local-1".into(),
            server_id: Some("p1".into()),
            kind: "post".into(),
            status: "public".into(),
            version: 2,
            dirty: false,
            created: "2026-10-01T00:00:00Z".into(),
            updated: "2026-10-02T00:00:00Z".into(),
            permalink: Some("https://blyg.example.com/p/1".into()),
            title: "Tides".into(),
            content_hash: "sha256:00".into(),
            content_md: md.map(str::to_string),
        }
    }

    fn params(md: Option<&str>) -> MacroPrepareParams {
        MacroPrepareParams {
            macro_id: MACRO.into(),
            item: item(md),
            permalink: "https://blyg.example.com/p/1".into(),
            text: "from the host\n\nhttps://blyg.example.com/p/1".into(),
        }
    }

    fn with(pairs: &[(&str, &str)]) -> CrossPostExt {
        let mut e = CrossPostExt::default();
        e.configure(
            &pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        e
    }

    #[test]
    fn prepares_the_opening_and_the_link() {
        let r = CrossPostExt::default()
            .prepare(&params(Some("# Tides\n\nThe moon pulls the sea.\n")))
            .unwrap();
        assert_eq!(
            r.text,
            "The moon pulls the sea.\n\nhttps://blyg.example.com/p/1"
        );
        assert_eq!(r.note, None);
        let r = CrossPostExt::default().prepare(&params(None)).unwrap();
        assert_eq!(r.text, "from the host\n\nhttps://blyg.example.com/p/1");
    }

    #[test]
    fn settings_shape_it_and_bad_ones_say_so() {
        let e = with(&[
            ("template", "{{title}}\\n{{permalink}}"),
            ("max-chars", "500"),
        ]);
        let r = e.prepare(&params(Some("Body."))).unwrap();
        assert_eq!(r.text, "Tides\nhttps://blyg.example.com/p/1");
        let e = with(&[("template", "{{nope}}"), ("max-chars", "zero")]);
        let r = e.prepare(&params(Some("Body."))).unwrap();
        assert_eq!(r.text, "Body.\n\nhttps://blyg.example.com/p/1");
        let note = r.note.unwrap();
        assert!(
            note.contains("{{nope}}") && note.contains("max-chars"),
            "{note}"
        );
    }

    #[test]
    fn each_site_gets_its_own_limit() {
        let long = "tide ".repeat(100);
        let mut p = params(Some(&long));
        let e = CrossPostExt::default();
        let substack = e.prepare(&p).unwrap();
        assert!(substack.text.chars().count() <= 280);
        assert!(substack.note.unwrap().contains("(max-chars)"));
        p.macro_id = BLUESKY_MACRO.into();
        let bluesky = e.prepare(&p).unwrap();
        let n = bluesky.text.chars().count();
        assert!(n <= 300 && n > 280, "{n}");
        assert_eq!(
            bluesky.note.as_deref(),
            Some("Cut to 300 characters (bluesky-max-chars)")
        );
        p.macro_id = FARCASTER_MACRO.into();
        let cast = e.prepare(&p).unwrap();
        assert!(cast.text.len() <= 320 && cast.text.len() > 300);
        assert_eq!(
            cast.note.as_deref(),
            Some("Cut to 320 bytes (farcaster-max-bytes)")
        );
        assert!(cast.text.ends_with("\n\nhttps://blyg.example.com/p/1"));
    }

    #[test]
    fn the_new_limits_are_settings() {
        let e = with(&[("bluesky-max-chars", "60"), ("farcaster-max-bytes", "1024")]);
        let long = "tide ".repeat(400);
        let mut p = params(Some(&long));
        p.macro_id = BLUESKY_MACRO.into();
        assert!(e.prepare(&p).unwrap().text.chars().count() <= 60);
        p.macro_id = FARCASTER_MACRO.into();
        let cast = e.prepare(&p).unwrap().text;
        assert!(cast.len() <= 1024 && cast.len() > 1000, "{}", cast.len());
        let e = with(&[("farcaster-max-bytes", "lots")]);
        let r = e.prepare(&p).unwrap();
        assert!(r.text.len() <= 320);
        assert!(r.note.unwrap().contains("farcaster-max-bytes \"lots\""));
    }

    #[test]
    fn another_macro_is_refused() {
        let mut p = params(Some("x"));
        p.macro_id = "other".into();
        let e = CrossPostExt::default().prepare(&p).unwrap_err();
        assert_eq!(e.code, codes::INVALID_PARAMS);
    }
}
