//! Studio 0.64 e2e: maths (studio 0.62, decision #105). A formula publishes
//! as MathML with its TeX in an x-tex annotation; money stays prose; a
//! formula that doesn't parse refuses publish naming it; and a formula in
//! someone else's post reaches the reader redrawn from its annotation.

use super::*;

/// The MathML of each top-level formula in `html`.
fn formulas(html: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("<math") {
        let end = rest[at..].find("</math>").expect("closed") + at + "</math>".len();
        out.push(&rest[at..end]);
        rest = &rest[end..];
    }
    out
}

#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn formulas_publish_as_mathml_and_money_stays_prose() {
    let e = e2e();
    let dir = tempfile::tempdir().unwrap();
    let b = manual(dir.path());
    let t = tag("maths");
    let text = format!(
        "{t} area $x^2$ and\n\n$$\\frac{{a}}{{b}}$$\n\nIt costs $5 and $10.\n\nIn code: `$y$`."
    );
    // Burrow's preview finds the same formulas the studio publishes.
    let preview = blyg_render::render_markdown(&text);
    assert_eq!(formulas(&preview).len(), 2, "{preview}");
    let (id, sid) = published(&b, Kind::Fragment, &text, None);
    let doc = public_json(&format!("{}/items/{sid}.json", e.url));
    let html = doc["content_html"].as_str().unwrap();
    let f = formulas(html);
    assert_eq!(f.len(), 2, "{html}");
    assert!(
        f[0].contains("<annotation encoding=\"application/x-tex\">x^2</annotation>"),
        "{html}"
    );
    assert!(f[1].contains("display=\"block\""), "{html}");
    assert!(f[1].contains("\\frac{a}{b}</annotation>"), "{html}");
    assert!(
        html.contains("It costs $5 and $10"),
        "money is prose: {html}"
    );
    assert!(html.contains("<code>$y$</code>"), "code is code: {html}");
    // The studio's own MathML passes the allowlist Burrow redraws into.
    for m in &f {
        assert!(blyg_render::math::allowed_math(m), "{m}");
    }
    let _ = id;
}

#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn a_formula_that_does_not_parse_refuses_publish_naming_it() {
    e2e();
    let dir = tempfile::tempdir().unwrap();
    let b = manual(dir.path());
    let t = tag("badtex");
    let text = format!("{t} fine $x$ but $\\frac{{$ here");
    // The preview refuses it too, before anything is sent.
    let preview = blyg_render::render_preview(
        &text,
        blyg_render::Kind::Fragment,
        &blyg_render::NoResolver,
        &blyg_render::RenderOpts::default(),
    );
    assert_eq!(preview.stats.math_errors.len(), 1);
    assert_eq!(preview.stats.math_errors[0].formula, "$\\frac{$");
    let id = b.create_draft(Kind::Fragment, &text).unwrap();
    b.sync_now().unwrap();
    let Err(err) = b.publish(&id, None) else {
        panic!("a bad formula blocks publish")
    };
    let msg = err.to_string();
    eprintln!("publish refused: {msg}");
    assert!(msg.contains("$\\frac{$"), "names the formula: {msg}");
    match &err {
        CoreError::Rejected { status, .. } => assert_eq!(*status, 400),
        other => panic!("{other:?}"),
    }
    b.delete_draft(&id).ok();
}

#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn a_formula_in_someone_elses_post_is_redrawn_for_the_reader() {
    e2e();
    let src_url = url_b();
    let (d1, d2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let me = manual(d1.path());
    let source = backend_at(d2.path(), &src_url, cred_b(), false);
    let t = tag("readmaths");
    let (sid_local, rid) = published(
        &source,
        Kind::Fragment,
        &format!(
            "{t} Euler: $e^{{i\\pi}} + 1 = 0$, and $$\\sum_{{n=1}}^\\infty \\frac{{1}}{{n^2}}$$"
        ),
        None,
    );
    let sub = me.subscribe(&src_url, Some("Maths")).unwrap();
    let _unsub = Unsub(&me, sub.id.clone());
    sync_until_held(&me, &[&rid]);
    let r = reading_item(&me, &rid).expect("held");
    // The reader's maths step (`blyg_render::math::hole_math`, which the
    // app's `sanitize_at` runs around ammonia): every formula comes back
    // drawn by Burrow from its annotation, never the publisher's MathML.
    let holes = blyg_render::math::hole_math(&r.content_html);
    assert!(!holes.html().contains("<math"), "{}", holes.html());
    let out = holes.fill(holes.html());
    let f = formulas(&out);
    assert_eq!(f.len(), 2, "{out}");
    assert!(f[0].contains("e^{i\\pi} + 1 = 0</annotation>"), "{out}");
    assert!(f[1].starts_with("<math display=\"block\">"), "{out}");
    for m in &f {
        assert!(blyg_render::math::allowed_math(m), "{m}");
        let drawn = blyg_render::math::render_tex(
            &m[m.find("x-tex\">").unwrap() + 7..m.find("</annotation>").unwrap()]
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&"),
            m.contains("display=\"block\""),
        );
        assert_eq!(&drawn.html, m, "our own drawing");
    }
    me.unsubscribe(&sub.id).unwrap();
    let _ = sid_local;
}

#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn hostile_math_yields_nothing_executable() {
    // Not published by a studio (it would never write these): the reader's
    // maths step on publisher HTML an attacker controls.
    e2e();
    for hostile in [
        "<p>a<math><mi onclick=\"alert(1)\">x</mi><script>alert(2)</script></math>b</p>",
        "<math><semantics><mi>x</mi><annotation encoding=\"application/x-tex\">x&lt;/annotation&gt;&lt;script&gt;alert(1)&lt;/script&gt;</annotation></semantics></math>",
        "<math><annotation encoding=\"application/x-tex\">\\text{&quot; onmouseover=&quot;alert(1)}</annotation></math>",
        "<math><annotation encoding=\"application/x-tex\">x</annotation></math><img src=x onerror=alert(1)>",
    ] {
        let holes = blyg_render::math::hole_math(hostile);
        let out = holes.fill(holes.html());
        for m in formulas(&out) {
            assert!(blyg_render::math::allowed_math(m), "{hostile} → {out}");
            assert!(!m.contains("<script"), "{hostile} → {out}");
        }
        assert!(!out.contains("<mi onclick"), "{hostile} → {out}");
    }
}
