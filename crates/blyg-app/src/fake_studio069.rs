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

// --- agentic writer --- the fake blyg's `PUT /api/agentic/default`.

/// The Agentic feed as the fake blyg holds it: the sample edition until
/// one is written (or it's cleared, for the empty tab).
#[derive(Debug, Clone)]
pub struct AgenticState {
    /// The edition written last: its revision, what was sent, and when.
    pub written: Option<(u64, blyg_core::agentic_write::EditionInput, String)>,
    /// No edition at all (revision 0), until one is written.
    pub cleared: bool,
    /// This sign-in may write one (`putAgentic`).
    pub writable: bool,
    /// Editions written.
    pub puts: usize,
}

impl Default for AgenticState {
    fn default() -> Self {
        AgenticState {
            written: None,
            cleared: false,
            writable: true,
            puts: 0,
        }
    }
}

/// The sample edition's revision.
const SAMPLE_REVISION: u64 = 7;

impl AgenticState {
    pub fn revision(&self) -> u64 {
        match (&self.written, self.cleared) {
            (Some((r, _, _)), _) => *r,
            (None, true) => 0,
            (None, false) => SAMPLE_REVISION,
        }
    }

    /// The edition as the blyg serves it: references resolved against the
    /// held rows, the ones that don't resolve left out (as the studio does).
    pub fn edition(&self, rows: &[ReadingItem], title: &str) -> Edition {
        let Some((revision, input, at)) = &self.written else {
            if self.cleared {
                return Edition {
                    revision: 0,
                    updated_at: None,
                    mode: None,
                    layout: None,
                    draft: None,
                };
            }
            return edition(rows, title);
        };
        let held = |sub: &str, id: &str| {
            rows.iter()
                .find(|r| r.subscription_id.as_str() == sub && r.remote_id.as_str() == id)
        };
        let sections = input
            .sections
            .iter()
            .map(|s| Section {
                title: s.title.clone(),
                kind: s.kind.unwrap_or_default(),
                note: s.note.clone(),
                entries: s
                    .entries
                    .iter()
                    .filter_map(|p| {
                        let r = held(&p.sub, &p.remote_id)?;
                        Some(Placed {
                            sub: p.sub.clone(),
                            remote_id: p.remote_id.clone(),
                            version: p.version,
                            held_version: Some(r.version),
                            headline: p.headline.clone(),
                            summary: p.summary.clone(),
                            reason: p.reason.clone(),
                            present: p.present.unwrap_or_default(),
                            group: p.group.clone(),
                        })
                    })
                    .collect::<Vec<_>>(),
            })
            .filter(|s| !s.entries.is_empty())
            .collect();
        Edition {
            revision: *revision,
            updated_at: Some(at.clone()),
            mode: Some(Mode::Sections),
            layout: Some(Layout {
                title: input.title.clone(),
                lede: input.lede.clone(),
                agent: Agent {
                    name: input.agent.name.clone(),
                    model: input.agent.model.clone(),
                },
                generated_at: input.generated_at.clone(),
                sections,
            }),
            draft: None,
        }
    }

    /// `PUT`: from the revision it's at, else a conflict.
    pub fn put(
        &mut self,
        rows: &[ReadingItem],
        title: &str,
        expected: u64,
        input: &blyg_core::agentic_write::EditionInput,
    ) -> blyg_core::agentic_write::Put {
        use blyg_core::agentic_write::Put;
        if !self.writable {
            return Put::NotOffered;
        }
        let now = self.revision();
        if expected != now {
            return Put::Conflict;
        }
        self.written = Some((now + 1, input.clone(), chrono::Utc::now().to_rfc3339()));
        self.puts += 1;
        Put::Written(self.edition(rows, title))
    }
}

/// The blyg's Catch up summary.
pub fn catch_up_summary() -> CatchUpSummary {
    CatchUpSummary {
        summary: "Rue asks whether quotes outlive links. Ada answers that a quote also freezes \
                  the author's words; Lin says pins decide what may be frozen; and Rue ends \
                  back at links, since pins don't rot."
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
