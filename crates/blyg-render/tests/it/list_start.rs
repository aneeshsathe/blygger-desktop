//! An ordered list keeps its first number through the thread preview's
//! sanitizer (studio 0.69's `sanitizeHtml`, decision #114, spec §10.2 and
//! §16.15: "a reader's sanitizer SHOULD keep an integer start on ol"; the
//! blygger-spec reader case `whole-list-start-kept`). An integer only;
//! anything else is dropped.

use blyg_render::{Found, ItemKind, Kind, RenderOpts, Resolution, Resolver, render_preview};

const SRC: &str = "5k5k5k5k5k5k5k5k5k5k5k5k5k";

struct Remote(&'static str);

impl Resolver for Remote {
    fn resolve(&self, id: &str) -> Resolution {
        Resolution::Found(Found {
            origin: Some("https://remote.example/".into()),
            id: id.into(),
            version: 1,
            kind: ItemKind::Fragment,
            content_html: self.0.into(),
            author: None,
            page: None,
        })
    }
}

fn thread(md: &str, quoted: &'static str) -> String {
    let opts = RenderOpts {
        data_line: false,
        provenance: false,
        ..Default::default()
    };
    render_preview(md, Kind::Thread, &Remote(quoted), &opts).html
}

#[test]
fn a_quoted_list_keeps_its_integer_start() {
    let html = thread(
        &format!("![[{SRC}]]"),
        "<ol start=\"7\">\n<li>seventh</li>\n<li>eighth</li>\n</ol>\n",
    );
    assert!(html.contains("<ol start=\"7\">"), "{html}");
}

#[test]
fn a_threads_own_list_keeps_its_start() {
    let html = thread("3. three\n4. four\n", "");
    assert!(html.contains("<ol start=\"3\">"), "{html}");
}

#[test]
fn a_start_that_is_not_an_integer_is_dropped() {
    let html = thread(
        &format!("![[{SRC}]]"),
        "<ol start=\"x\"><li>a</li></ol><ol start=\"-2\"><li>b</li></ol><ul start=\"4\"><li>c</li></ul>",
    );
    assert!(html.contains("<ol><li>a</li></ol>"), "{html}");
    assert!(html.contains("<ol start=\"-2\">"), "{html}");
    assert!(html.contains("<ul><li>c</li></ul>"), "{html}");
}
