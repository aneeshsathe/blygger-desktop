//! Picker sources over the wire (studio 0.71's `pickerSource`): the host
//! against a real extension process (`bxp-picker-ext`). A manifest's
//! `[[picker-sources]]` become tabs for the forms they name, and
//! `extension/picker.search` answers come back cleaned as the studio's
//! `pickerResults` does: item ids only, each once, on one line, at most 50.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use blyg_ext::picker::{MAX_RESULTS, PickerForm, PickerResult, PickerSourceSpec, PickerTab};
use blyg_ext::*;

const NAME: &str = "bxp-picker";
const WAIT: Duration = Duration::from_secs(15);

fn manifest() -> String {
    let exe = env!("CARGO_BIN_EXE_bxp-picker-ext");
    format!(
        r#"name = "{NAME}"
version = "0.0.1"
protocol = 1
command = ['{exe}']

[[picker-sources]]
id = "refs"
label = "references"

[[picker-sources]]
id = "glossary"
label = "glossary"
forms = ["link", "transclude", "source"]
"#
    )
}

struct Env {
    _dir: tempfile::TempDir,
    host: Host,
    events: Receiver<ExtEvent>,
}

fn setup() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let ext_dir = dir.path().join("extensions").join(NAME);
    std::fs::create_dir_all(&ext_dir).unwrap();
    std::fs::write(ext_dir.join(MANIFEST_FILE), manifest()).unwrap();
    let mut c = HostConfig::new(dir.path().join("data"), "0.0.0-test");
    c.enabled = vec![NAME.into()];
    c.extensions_dir = Some(dir.path().join("extensions"));
    c.timing.shutdown = Duration::from_secs(5);
    let (tx, events) = channel();
    let host = Host::new(c, Arc::new(NoBlyg), move |e| {
        let _ = tx.send(e);
    });
    host.start();
    Env {
        _dir: dir,
        host,
        events,
    }
}

impl Env {
    fn started(&self) {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(ExtEvent::Started { .. }) => return,
                Ok(_) => continue,
                Err(_) => panic!("not started within {WAIT:?}"),
            }
        }
    }
}

const A: &str = "0123456789abcdefghjkmnpqrs";
const B: &str = "zzzzzzzzzzzzzzzzzzzzzzzzzz";

#[test]
fn a_manifest_declares_picker_tabs_by_form() {
    let m = Manifest::parse(&manifest(), false).unwrap();
    assert_eq!(
        m.picker_sources,
        [
            PickerSourceSpec {
                id: "refs".into(),
                label: "references".into(),
                forms: vec![PickerForm::Link],
            },
            PickerSourceSpec {
                id: "glossary".into(),
                label: "glossary".into(),
                forms: vec![PickerForm::Link, PickerForm::Transclude, PickerForm::Source],
            },
        ],
        "forms default to link only"
    );
    let bad = manifest().replace("label = \"references\"", "label = \"  \"");
    assert!(Manifest::parse(&bad, false).is_err(), "a tab needs a label");
    let bad = manifest().replace("\"source\"]", "\"sauce\"]");
    assert!(Manifest::parse(&bad, false).is_err(), "an unknown form");
}

#[test]
fn tabs_are_offered_for_their_forms_and_answer_cleaned() {
    let env = setup();
    env.started();
    let tabs = |f| -> Vec<(String, String)> {
        env.host
            .picker_tabs(f)
            .into_iter()
            .map(|PickerTab { ext, source }| (ext, source.id))
            .collect()
    };
    assert_eq!(
        tabs(PickerForm::Link),
        [
            (NAME.into(), "refs".into()),
            (NAME.into(), "glossary".into())
        ]
    );
    assert_eq!(
        tabs(PickerForm::Transclude),
        [(NAME.into(), "glossary".into())]
    );

    // The query and form cross; the answer is cleaned (see bxp-picker-ext).
    let r = env
        .host
        .picker_search(NAME, "refs", "polanyi", PickerForm::Link)
        .unwrap();
    assert_eq!(
        r[0],
        PickerResult {
            id: A.into(),
            label: Some("Polanyi 1966".into()),
            excerpt: "Polanyi, Michael. The Tacit Dimension.".into(),
            meta: Some("refs · link · polanyi".into()),
        },
        "spaces folded to one line"
    );
    assert_eq!(
        r[1],
        PickerResult {
            id: B.into(),
            label: None,
            excerpt: B.into(),
            meta: None,
        },
        "no excerpt or label: the id stands in"
    );
    assert_eq!(
        r.len(),
        2,
        "not an item id, and a repeat, are dropped: {r:?}"
    );

    let many = env
        .host
        .picker_search(NAME, "refs", "many", PickerForm::Link)
        .unwrap();
    assert_eq!(many.len(), MAX_RESULTS);

    // A source not offered for the form isn't asked.
    assert!(
        env.host
            .picker_search(NAME, "refs", "x", PickerForm::Transclude)
            .is_err()
    );
    // The extension's own error comes back as an error.
    assert!(
        env.host
            .picker_search(NAME, "refs", "broken", PickerForm::Link)
            .is_err()
    );
}

#[test]
fn results_are_item_ids_once_on_one_line() {
    use serde_json::json;
    let raw = json!([
        {"id": A, "label": " Two\n words ", "excerpt": "", "meta": "  "},
        {"id": A, "label": "again"},
        {"id": "not-an-id"},
        {"label": "no id"},
        "a string",
        {"id": B, "excerpt": "an\texcerpt"},
    ]);
    assert_eq!(
        blyg_ext::picker::results(raw),
        [
            PickerResult {
                id: A.into(),
                label: Some("Two words".into()),
                excerpt: "Two words".into(),
                meta: None,
            },
            PickerResult {
                id: B.into(),
                label: None,
                excerpt: "an excerpt".into(),
                meta: None,
            },
        ]
    );
    assert!(blyg_ext::picker::results(json!({"id": A})).is_empty());
}
