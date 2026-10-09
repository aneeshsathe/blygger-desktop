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

use crate::text::{max_chars_setting, prepare, template_setting};
use crate::{MACRO, NAME};

/// The extension's state: its settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossPostExt {
    template: String,
    max_chars: usize,
    /// Settings it couldn't use (the default stands), for the preview
    /// sheet's note.
    problems: Vec<String>,
}

impl Default for CrossPostExt {
    fn default() -> Self {
        let mut e = CrossPostExt {
            template: String::new(),
            max_chars: 0,
            problems: vec![],
        };
        e.configure(&BTreeMap::new());
        e
    }
}

impl CrossPostExt {
    /// Read `template` and `max-chars` (`extension-setting = cross-post …`).
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
    }

    /// `extension/macro.prepare`.
    pub fn prepare(&self, p: &MacroPrepareParams) -> Result<MacroPrepareResult, RpcError> {
        if p.macro_id != MACRO {
            return Err(RpcError::invalid_params(format!(
                "{NAME} has no macro {:?}",
                p.macro_id
            )));
        }
        let out = prepare(
            p.item.content_md.as_deref(),
            &p.item.title,
            &p.permalink,
            &self.template,
            self.max_chars,
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
    fn another_macro_is_refused() {
        let mut p = params(Some("x"));
        p.macro_id = "other".into();
        let e = CrossPostExt::default().prepare(&p).unwrap_err();
        assert_eq!(e.code, codes::INVALID_PARAMS);
    }
}
