//! --- render --- The preview of studio 0.64.1's rendering changes
//! (`BLYGGER_DEMO=rd-provenance`): a draft thread quoting Ada's thread,
//! which itself carries a labelled quote of Rue's (a nested provenance
//! line), a label on each quote (the label anchors the line), a partial
//! quote with a lazy continuation line, and comments to self (left out of
//! the preview).

use blyg_core::{Backend, Kind, LocalId, ReadingItem};
use chrono::{DateTime, Duration, Utc};

use super::reading_seed::{ADA, RUE, Seed, TALK_A};

/// Ada's thread that quotes Rue's: its baked quote carries a label.
pub const ADA_TOOLS: &str = "01k2ada0t00150000000000001";

/// Ada's thread, as her blyg serves it (the quote already baked).
pub fn add(rd: &mut Seed, now: DateTime<Utc>) {
    let Some(template) = rd
        .reading
        .iter()
        .find(|r| &*r.subscription_id == "sub-ada")
        .cloned()
    else {
        return;
    };
    // Older than the rest, so the stream and its tests open as before.
    let at = (now - Duration::days(90)).to_rfc3339();
    let md = format!(
        "On tools\n\nRue asked this first:\n\n![[{TALK_A} | Rue's question]]\n\nMy answer: quote the pin."
    );
    rd.reading.push(ReadingItem {
        remote_id: ADA_TOOLS.into(),
        kind: Kind::Thread,
        version: 2,
        created: Some(at.clone()),
        updated: Some(at.clone()),
        observed_at: at,
        content_md: md,
        content_html: format!(
            "<p>On tools</p>\n<p>Rue asked this first:</p>\n\
             <blockquote class=\"blyg-transclusion\" data-blyg-id=\"{TALK_A}\" data-blyg-version=\"1\" \
             data-blyg-origin=\"{RUE}\" data-blyg-label=\"Rue&#39;s question\">\
             <p>Do quotes outlive links?</p></blockquote>\n<p>My answer: quote the pin.</p>\n"
        ),
        page: Some(format!("t/{ADA_TOOLS}")),
        origin: ADA.into(),
        read_version: None,
        ..template
    });
}

/// The draft that shows the changes, opened in the full editor.
pub fn seed(backend: &dyn Backend) -> Option<LocalId> {
    let draft = format!(
        "Tools and tides\n\n\
         I wrote about tools before /* link the workshop post too */ and quote Ada here.\n\n\
         ![[{ADA_TOOLS} | Ada on tools]]\n\n\
         /* a whole paragraph of notes to self,\n\
         left out of the preview */\n\n\
         ![[{} | Shoreline's line]]\n\
         > A tide pool is a small ocean that forgets,\n\
         twice a day, that it belongs to a larger one.\n\n\
         The same forgetting, in both.\n",
        crate::fake::SAMPLE_QUOTE_ID
    );
    backend.create_draft(Kind::Thread, &draft).ok()
}
