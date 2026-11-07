//! --- trails --- a path through posts you chose to keep: an ordered list of
//! stops (your published posts and posts you hold from others), each with a
//! one-line "why". Local to this Mac (store schema v19), never on the wire.
//! Publishing one writes an ordinary thread of quotes (`publish_text`); the
//! user publishes it like any other draft.

use serde::{Deserialize, Serialize};

/// A trail, as the sources pane lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trail {
    pub id: String,
    pub name: String,
    /// How many stops it has (tombstones included).
    pub stops: usize,
}

/// One stop on a trail. Keyed like a reading row: `(subscription_id,
/// remote_id)`, with `crate::OWN_SUBSCRIPTION` and the server id for a post
/// of yours.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrailStop {
    pub subscription_id: String,
    pub remote_id: String,
    pub origin: String,
    /// Why it's on the trail, in the user's words (may be empty).
    pub why: String,
    /// The post's title as it was when the stop was added; `None` once the
    /// post is withdrawn (its text is dropped, rule 5).
    pub title: Option<String>,
    /// The post was withdrawn: the stop is a tombstone.
    pub withdrawn: bool,
}

impl TrailStop {
    pub fn key(&self) -> (String, String) {
        (self.subscription_id.clone(), self.remote_id.clone())
    }
}

/// The most stops a trail holds (a thread of quotes stays readable).
pub const TRAIL_MAX_STOPS: usize = 200;

/// The thread a trail publishes as: `# name`, then per stop
/// `![[id | why]]` and a blank line (the stop's title when there's no why;
/// bare below studio 0.47, `below`). Withdrawn stops are left out: a
/// withdrawn post is never quoted again.
pub fn publish_text(name: &str, stops: &[TrailStop], below: bool) -> String {
    let mut out = format!("# {}\n\n", name.trim());
    for s in stops.iter().filter(|s| !s.withdrawn) {
        let why = s.why.trim();
        let label = if why.is_empty() {
            s.title.as_deref().map(str::trim).unwrap_or("")
        } else {
            why
        };
        let label = (!label.is_empty()).then_some(label);
        out.push_str(&crate::grammar::directive_source(
            &s.remote_id,
            crate::floor::picker_label(below, label.unwrap_or("")).and(label),
        ));
        out.push_str("\n\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(id: &str, why: &str, title: Option<&str>, withdrawn: bool) -> TrailStop {
        TrailStop {
            subscription_id: "s1".into(),
            remote_id: id.into(),
            origin: "https://blyg.example.com/".into(),
            why: why.into(),
            title: title.map(str::to_string),
            withdrawn,
        }
    }

    #[test]
    fn a_trail_publishes_as_a_thread_of_quotes() {
        let stops = [
            stop("aaa", "the tide sets the budget", Some("Rock pools"), false),
            stop("bbb", "", Some("Weeding"), false),
            stop("ccc", "gone now", None, true),
            stop("ddd", "", None, false),
        ];
        assert_eq!(
            publish_text(" weeding ", &stops, false),
            "# weeding\n\n![[aaa | the tide sets the budget]]\n\n![[bbb | Weeding]]\n\n![[ddd]]\n\n"
        );
        // Below studio 0.47 the quotes are bare.
        assert_eq!(
            publish_text("w", &stops, true),
            "# w\n\n![[aaa]]\n\n![[bbb]]\n\n![[ddd]]\n\n"
        );
    }
}
