//! --- studio 0.71 --- the fake blyg's generate (`BLYGGER_FAKE=1`): the
//! links of a `[TK]` instruction are "read" first, as studio 0.70 does. A
//! link on a `.invalid` host can't be read, so generation stops at a
//! checkpoint until `proceed`; every other web link is read and disclosed
//! as a web-page source. Invented content.

use blyg_core::generate::{Checkpoint, GenerateOutcome, Generated, UnreadLink};
use blyg_core::{CoreError, PageSource, ScopeProvenance};

/// The fake blyg's model.
pub const MODEL: &str = "fake-blyg-model";

/// What one generate does to `md`: the outcome, and on `Generated` the
/// spliced text and the scope's provenance.
pub struct Filled {
    pub outcome: GenerateOutcome,
    pub spliced: Option<(String, ScopeProvenance)>,
}

pub fn generate(
    md: &str,
    scope: usize,
    proceed: bool,
    revision: u64,
    now: &str,
) -> Result<Filled, CoreError> {
    let (scopes, errors) = blyg_ai::prompts::parse_scopes(md);
    let bad = |m: &str| CoreError::Rejected {
        status: 400,
        message: m.into(),
        details: vec![],
    };
    if !errors.is_empty() {
        return Err(bad("working copy has malformed TK scopes"));
    }
    let s = scopes
        .get(scope)
        .ok_or_else(|| bad("unknown scope index"))?;
    let links: Vec<&str> = s
        .instruction
        .split_whitespace()
        .filter(|w| w.starts_with("http://") || w.starts_with("https://"))
        .collect();
    let unreadable = |l: &str| {
        url::Url::parse(l)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.ends_with(".invalid")))
            .unwrap_or(true)
    };
    let failed: Vec<UnreadLink> = links
        .iter()
        .filter(|l| unreadable(l))
        .map(|l| UnreadLink {
            link: l.to_string(),
            reason: "HTTP 404".into(),
        })
        .collect();
    if !failed.is_empty() && !proceed {
        return Ok(Filled {
            outcome: GenerateOutcome::Checkpoint(Checkpoint {
                read: (links.len() - failed.len()) as u32,
                total: links.len() as u32,
                failed,
            }),
            spliced: None,
        });
    }
    let pages = links
        .iter()
        .filter(|l| !unreadable(l))
        .map(|l| PageSource {
            url: l.to_string(),
            cited: serde_json::json!({
                "source": url::Url::parse(l).ok().and_then(|u| u.host_str().map(str::to_string)),
                "url": l,
                "retrieved": now,
            }),
        })
        .collect();
    let text = "A line the fake blyg wrote.".to_string();
    let spliced = blyg_ai::prompts::set_scope_output(md, s, &text);
    Ok(Filled {
        outcome: GenerateOutcome::Generated(Generated {
            text,
            model: MODEL.into(),
            content_md: Some(spliced.clone()),
            revision: revision + 1,
        }),
        spliced: Some((
            spliced,
            ScopeProvenance {
                model: MODEL.into(),
                sources: vec![],
                pages,
                at: Some(now.into()),
            },
        )),
    })
}
