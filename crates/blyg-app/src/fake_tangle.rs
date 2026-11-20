//! --- tangle --- The sample backend answers a thread's `![[+ …]]` spans as
//! studio 0.63 does: a publish without `tangle: true` is refused with the
//! count and the parts' names (`CoreError::TangleConfirm`); with it, each
//! span is rewritten to a labelled reference to a new item, and the
//! outcome carries the rewrite. The parts themselves aren't added to the
//! sample (the `rd-tangle` scene stops at the confirm step).

use blyg_core::{Backend, CoreError, Kind, LocalId, Result};
use blyg_render::tangle::{SpanKind, confirm_labels, parse_tangles, tangle_label};

/// The thread the `rd-tangle` scene writes in.
pub const DEMO_THREAD: &str = "01J9K7T";

/// What the scene writes: a passage that becomes a new item, and a
/// placeholder for one not yet written.
pub const SAMPLE: &str = "Notes from the harbour walk.\n\n![[+ The tide tables were wrong by an hour. Nobody on the quay seemed to mind; the boats left when the water said so.]]\n\nNext time I'll start from the lighthouse, and write up [[+ the ferry timetable]] properly.\n";

/// The refusal a studio 0.63 gives an unconfirmed tangle, if `md` has one.
pub fn confirm(kind: Kind, md: &str) -> Option<CoreError> {
    let t = parse_tangles(md);
    (kind == Kind::Thread && !t.spans.is_empty() && t.issues.is_empty()).then(|| {
        CoreError::TangleConfirm {
            count: t.spans.len(),
            parts: confirm_labels(&t.spans),
        }
    })
}

/// Rewrite the post's spans as the studio does (`rewriteTangles`), save it,
/// and give back the text and the parts' invented ids.
pub fn tangle(b: &dyn Backend, id: &LocalId) -> Result<Option<(String, Vec<String>)>> {
    let Some(item) = b.item(id) else {
        return Ok(None);
    };
    let md = item.content_md;
    let spans = parse_tangles(&md).spans;
    if spans.is_empty() {
        return Ok(None);
    }
    let ids: Vec<String> = (0..spans.len())
        .map(|i| format!("01j9t4r7c8m2q5v6w3x8y9z0{i:02}"))
        .collect();
    let mut out = String::new();
    let mut last = 0;
    for (span, pid) in spans.iter().zip(&ids) {
        out.push_str(&md[last..span.start]);
        let l = tangle_label(span);
        match span.kind {
            SpanKind::Link => {
                out.push_str(&blyg_render::link_source(pid, Some(&span.text)));
                last = span.end;
            }
            SpanKind::Quote => {
                out.push_str(&if l.is_empty() {
                    format!("![[{pid}]]")
                } else {
                    format!("![[{pid} | {l}]]")
                });
                let mut rest = span.end;
                while md.as_bytes().get(rest) == Some(&b'\n') {
                    rest += 1;
                }
                out.push_str(if rest < md.len() { "\n\n" } else { "\n" });
                last = rest;
            }
        }
    }
    out.push_str(&md[last..]);
    b.save(id, &out)?;
    Ok(Some((out, ids)))
}
