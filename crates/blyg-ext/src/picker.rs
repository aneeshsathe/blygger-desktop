//! Picker sources: tabs an extension adds to the `[[` / `![[` picker,
//! beside Burrow's own search. They mirror the web Studio's `pickerSource`
//! (blygger-studio 0.71, `src/ui/extension-api.ts`, docs/extensions.md
//! "Picker sources"):
//!
//! ```toml
//! [[picker-sources]]
//! id = "refs"
//! label = "references"          # the tab's name
//! forms = ["link"]              # link | transclude | source (default: link)
//! ```
//!
//! The extension finds; Burrow does the rest. `extension/picker.search
//! {source, query, form}` (2 s) answers a list of results, and the picker
//! lists them, moves through them with the keys and inserts the one picked
//! in the form being typed (`[[id | label]]`). Results are item ids only,
//! each once, on one line, at most [`MAX_RESULTS`] ([`results`], the
//! studio's `pickerResults`). No capability: the extension sees only what is
//! typed after the brackets while its tab is open, and answers ids.
//! An optional method of protocol 1: an extension that declares no picker
//! source is never asked.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::host::{ExtError, Host};
use crate::protocol::{RpcError, codes};
use crate::rpc::to_value;

/// `extension/picker.search` (host -> extension, `Timing::search`):
/// [`PickerSearchParams`] -> a list of results.
pub const PICKER_SEARCH: &str = "extension/picker.search";

/// At most this many results a tab (the studio's `MAX_PICKER_RESULTS`).
pub const MAX_RESULTS: usize = 50;

/// What is being typed (the studio's `PickerForm`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PickerForm {
    /// `[[…]]`, a link.
    Link,
    /// `![[…]]` on a line of its own, a quote.
    Transclude,
    /// `![[…]]` inside a `[TK]` instruction, a source.
    Source,
}

fn link_only() -> Vec<PickerForm> {
    vec![PickerForm::Link]
}

/// A tab an extension adds (manifest `[[picker-sources]]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickerSourceSpec {
    /// Sent back in [`PickerSearchParams::source`].
    pub id: String,
    /// The tab's name.
    pub label: String,
    /// Where the tab is offered.
    #[serde(default = "link_only")]
    pub forms: Vec<PickerForm>,
}

/// One extension's tab, as the picker lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerTab {
    pub ext: String,
    pub source: PickerSourceSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickerSearchParams {
    pub source: String,
    /// What follows the brackets (or the picker's own search box).
    #[serde(default)]
    pub query: String,
    pub form: PickerForm,
}

/// One result, as the picker shows it: an item id, the line shown
/// (`excerpt`, else the label, else the id), the label a pick writes
/// (`[[id | label]]`), and a note on the right.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickerResult {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub excerpt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,
}

/// A source's answer as the picker can use it (the studio's
/// `pickerResults`): item ids only, each once, with a one-line excerpt and
/// label, at most [`MAX_RESULTS`]. Anything else is dropped.
pub fn results(answer: Value) -> Vec<PickerResult> {
    let Value::Array(rows) = answer else {
        return vec![];
    };
    let line = |v: Option<&Value>| -> String {
        v.and_then(Value::as_str)
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default()
    };
    let mut out: Vec<PickerResult> = vec![];
    for row in rows {
        let Some(id) = row.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !blyg_core::grammar::is_item_id(id) || out.iter().any(|r| r.id == id) {
            continue;
        }
        let label = line(row.get("label"));
        let meta = line(row.get("meta"));
        let excerpt = Some(line(row.get("excerpt")))
            .filter(|e| !e.is_empty())
            .or_else(|| Some(label.clone()).filter(|l| !l.is_empty()))
            .unwrap_or_else(|| id.to_string());
        out.push(PickerResult {
            id: id.to_string(),
            label: (!label.is_empty()).then_some(label),
            excerpt,
            meta: (!meta.is_empty()).then_some(meta),
        });
        if out.len() == MAX_RESULTS {
            break;
        }
    }
    out
}

impl Host {
    /// The tabs running extensions offer for `form`, by extension name then
    /// manifest order. Cheap (no call into an extension): the picker asks
    /// it as it opens.
    pub fn picker_tabs(&self, form: PickerForm) -> Vec<PickerTab> {
        self.running_manifests()
            .into_iter()
            .flat_map(|(ext, m, _)| {
                m.picker_sources
                    .into_iter()
                    .filter(move |s| s.forms.contains(&form))
                    .map(move |source| PickerTab {
                        ext: ext.clone(),
                        source,
                    })
            })
            .collect()
    }

    /// Ask `ext`'s source for `query` (blocking, up to `Timing::search`;
    /// run it off the main thread). A source not offered for `form` isn't
    /// asked.
    pub fn picker_search(
        &self,
        ext: &str,
        source: &str,
        query: &str,
        form: PickerForm,
    ) -> Result<Vec<PickerResult>, ExtError> {
        if !self
            .picker_tabs(form)
            .iter()
            .any(|t| t.ext == ext && t.source.id == source)
        {
            return Err(ExtError::Rpc(RpcError::new(
                codes::REFUSED,
                format!("{ext} offers no picker tab {source:?} here"),
            )));
        }
        let p = to_value(&PickerSearchParams {
            source: source.into(),
            query: query.into(),
            form,
        })
        .map_err(ExtError::Rpc)?;
        let answer: Value = self.request(ext, PICKER_SEARCH, p, self.timing().search)?;
        Ok(results(answer))
    }
}

/// The answers of the test extension (`bxp-picker-ext`, and blyg-app's
/// picker tab tests, which run the same from their own child process); see
/// that binary for what each query gives. Ids are invented.
pub fn sample_answer(p: &PickerSearchParams) -> Result<Value, String> {
    const A: &str = "0123456789abcdefghjkmnpqrs";
    const B: &str = "zzzzzzzzzzzzzzzzzzzzzzzzzz";
    match p.query.as_str() {
        "broken" => Err("the index is missing".into()),
        "many" => Ok(Value::Array(
            (0..60)
                .map(|n| json!({ "id": format!("{n:0>26}"), "label": format!("ref {n}") }))
                .collect(),
        )),
        q => Ok(json!([
            {
                "id": A,
                "label": "Polanyi  1966",
                "excerpt": "Polanyi,\n Michael.  The Tacit Dimension.",
                "meta": format!("{} · {} · {q}", p.source, form_name(p.form)),
            },
            { "id": B },
            { "id": "not-an-item" },
            { "id": A, "label": "again" },
        ])),
    }
}

fn form_name(f: PickerForm) -> &'static str {
    match f {
        PickerForm::Link => "link",
        PickerForm::Transclude => "transclude",
        PickerForm::Source => "source",
    }
}
