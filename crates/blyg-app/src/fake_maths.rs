//! --- maths --- Sample data for the maths scenes (studio 0.62): a post of
//! Ada's with formulas, as her blyg would publish it (MathML with each
//! formula's TeX in an x-tex annotation), and a draft of yours with one
//! formula that doesn't parse.

use blyg_core::*;
use chrono::{DateTime, Duration, Utc};

use super::reading_seed::Seed;

/// Ada's post with formulas (a Crockford id, 26 characters).
pub const ADA_MATHS: &str = "01k2adamath000000000000001";

/// A draft of yours with formulas, for the editor preview.
pub const DRAFT_MATHS: &str = "Area of a circle, $A = \\pi r^2$, and Basel's sum:\n\n$$\\sum_{n=1}^{\\infty} \\frac{1}{n^2} = \\frac{\\pi^2}{6}$$\n\nIt costs $5 and $10 to print.\n\nThis one doesn't parse: $\\frac{1}{$";

const ADA_TEXT: &str = "Euler's identity, $e^{i\\pi} + 1 = 0$, ties five constants together.\n\n$$\\int_0^1 x^2\\,dx = \\frac{1}{3}$$\n\nAnd money stays money: it cost $5 and $10.";

/// Add Ada's maths post to the reading list.
pub fn add(rd: &mut Seed, now: DateTime<Utc>) {
    let Some(template) = rd
        .reading
        .iter()
        .find(|r| &*r.subscription_id == "sub-ada")
        .cloned()
    else {
        return;
    };
    let at = (now - Duration::minutes(20)).to_rfc3339();
    rd.reading.push(ReadingItem {
        remote_id: ADA_MATHS.into(),
        kind: Kind::Fragment,
        version: 1,
        created: Some(at.clone()),
        updated: Some(at.clone()),
        observed_at: at,
        content_md: ADA_TEXT.into(),
        content_html: blyg_render::render_markdown(ADA_TEXT),
        page: Some(format!("f/{ADA_MATHS}")),
        read_version: None,
        ..template
    });
}
