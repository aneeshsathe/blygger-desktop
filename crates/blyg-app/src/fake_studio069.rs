//! --- studio 0.69 --- the fake blyg's Agentic feed edition, dictionary and
//! Catch up summary (`BLYGGER_FAKE=1`, demo scenes). Invented content.

use blyg_core::ReadingItem;
use blyg_core::agentic::{Agent, Edition, Layout, Mode, Placed, Present, Section, SectionKind};
use blyg_core::dictionary::{Dictionary, OWNER, Source};
use blyg_core::helpers::CatchUpSummary;

/// The fake's dictionary: a few words of yours and an installed set.
pub fn dictionary() -> Dictionary {
    Dictionary {
        revision: 3,
        sources: vec![
            Source {
                source: OWNER.into(),
                words: ["blyg", "blygs", "Burrow", "fragmentary", "tidepool"]
                    .map(String::from)
                    .to_vec(),
            },
            Source {
                source: "maritime".into(),
                words: ["bosun", "gunwale", "halyard"].map(String::from).to_vec(),
            },
        ],
        writable: true,
    }
}

/// An edition over the first held posts: a lead, a section and briefs.
pub fn edition(rows: &[ReadingItem], title: &str) -> Edition {
    let held: Vec<&ReadingItem> = rows
        .iter()
        .filter(|r| r.subscription_id.as_str() != "~own")
        .collect();
    let place = |r: &ReadingItem,
                 headline: Option<&str>,
                 summary: Option<&str>,
                 reason: Option<&str>,
                 present| {
        Placed {
            sub: r.subscription_id.clone(),
            remote_id: r.remote_id.clone(),
            version: Some(r.version),
            held_version: Some(r.version),
            headline: headline.map(str::to_string),
            summary: summary.map(str::to_string),
            reason: reason.map(str::to_string),
            present,
            group: None,
        }
    };
    let mut sections = Vec::new();
    if let Some(r) = held.first() {
        sections.push(Section {
            title: "Lead".into(),
            kind: SectionKind::Lead,
            note: None,
            entries: vec![place(
                r,
                Some("A slow week for the harbour, and a long look at why"),
                Some("Three posts circle the same question about what a quiet season is for; this one answers it best."),
                Some("You thumbed two earlier posts in this thread"),
                Present::Full,
            )],
        });
    }
    let middle: Vec<Placed> = held
        .iter()
        .skip(1)
        .take(3)
        .map(|r| {
            place(
                r,
                None,
                None,
                Some("New from a blyg you read daily"),
                Present::Excerpt,
            )
        })
        .collect();
    if !middle.is_empty() {
        sections.push(Section {
            title: "Worth your morning".into(),
            kind: SectionKind::Section,
            note: Some("Longer pieces, in the order they build on each other.".into()),
            entries: middle,
        });
    }
    let briefs: Vec<Placed> = held
        .iter()
        .skip(4)
        .take(4)
        .map(|r| place(r, None, None, None, Present::Title))
        .collect();
    if !briefs.is_empty() {
        sections.push(Section {
            title: "Briefly".into(),
            kind: SectionKind::Briefs,
            note: None,
            entries: briefs,
        });
    }
    Edition {
        revision: 7,
        updated_at: Some("2026-11-21T07:30:00Z".into()),
        mode: Some(Mode::Sections),
        layout: Some(Layout {
            title: Some(title.to_string()),
            lede: Some(
                "A quiet week: one long argument about harbours, a few short notes, and a recipe."
                    .into(),
            ),
            agent: Agent {
                name: "resident".into(),
                model: Some("invented-model-1".into()),
            },
            generated_at: Some("2026-11-21T07:30:00Z".into()),
            sections,
        }),
        draft: None,
    }
}

/// The blyg's Catch up summary.
pub fn catch_up_summary() -> CatchUpSummary {
    CatchUpSummary {
        summary: "The thread starts with a question about tide tables, gets a correction about \
                  spring tides, and ends with a shared chart everyone agrees on."
            .into(),
        model: "claude-haiku-4-5".into(),
        at: "2026-11-21T08:00:00Z".into(),
        sources: Vec::new(),
        cached: false,
    }
}

#[cfg(test)]
mod tests {
    /// The sample Catch up summary is about the sample thread it sums up
    /// (Rue's "Do quotes outlive links?"), not another subject.
    #[test]
    fn the_sample_summary_matches_the_sample_thread() {
        let s = super::catch_up_summary().summary.to_lowercase();
        assert!(s.contains("quote") && s.contains("link"), "{s}");
        assert!(!s.contains("tide"), "{s}");
    }
}
