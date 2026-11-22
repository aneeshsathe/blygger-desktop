//! Burrow's syndication wire shapes (`blyg_core::syndication`) against the
//! released contract: studio 0.67.0 (contract 6.0.0; spec 0.4 §16.14,
//! decision #98) shipped `getSyndication` and `putSyndication` at
//! `/api/items/{id}/syndication`, answering a `Syndication` of
//! `SyndicationEntry` rows; 0.67.1 put the `syndication` member on every
//! imported reading row. Burrow was built against a pre-release head, so
//! these hold its shapes to the vendored `openapi-current.json`.

use blyg_core::syndication::{self, SyndicationEntry};
use serde_json::{Value, json};

fn contract() -> Value {
    serde_json::from_str(include_str!("../fixtures/openapi-current.json")).expect("contract JSON")
}

fn schema<'a>(doc: &'a Value, name: &str) -> &'a Value {
    let s = &doc["components"]["schemas"][name];
    assert!(s.is_object(), "no schema {name} in the contract");
    s
}

/// An object schema's property names, sorted.
fn props(s: &Value) -> Vec<String> {
    let mut k: Vec<String> = s["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .cloned()
        .collect();
    k.sort();
    k
}

fn required(s: &Value) -> Vec<String> {
    let mut k: Vec<String> = s["required"]
        .as_array()
        .expect("required")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    k.sort();
    k
}

/// A JSON object's keys, sorted.
fn keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().expect("an object").keys().cloned().collect();
    k.sort();
    k
}

fn json_schema<'a>(op: &'a Value, status: &str) -> &'a Value {
    &op["responses"][status]["content"]["application/json"]["schema"]
}

#[test]
fn the_route_and_its_operations_are_the_released_ones() {
    let doc = contract();
    let path = &doc["paths"]["/items/{id}/syndication"];
    assert_eq!(path["get"]["operationId"], "getSyndication");
    assert_eq!(path["put"]["operationId"], "putSyndication");
    // Gated on the write's operation id, never the contract number.
    assert_eq!(syndication::PUT_OPERATION, "putSyndication");
    assert!(syndication::serves(
        &json!({ "operations": ["getSyndication", "putSyndication"] })
    ));
    assert!(!syndication::serves(&json!({ "operations": ["getItem"] })));
    // `/api` is the server's base in the contract.
    assert_eq!(
        syndication::route("01k2abc"),
        "/api/items/01k2abc/syndication"
    );
    for op in [&path["get"], &path["put"]] {
        assert_eq!(
            json_schema(op, "200")["$ref"],
            "#/components/schemas/Syndication"
        );
    }
}

#[test]
fn an_entry_is_a_url_and_a_version_and_nothing_else() {
    let doc = contract();
    let e = schema(&doc, "SyndicationEntry");
    assert_eq!(props(e), ["url", "version"]);
    assert_eq!(required(e), ["url", "version"]);
    assert_eq!(e["additionalProperties"], false);
    assert_eq!(e["properties"]["url"]["maxLength"], syndication::MAX_URL);
    let ours = serde_json::to_value(SyndicationEntry::new(
        "https://social.example.net/@burrow/1",
        2,
    ))
    .unwrap();
    assert_eq!(keys(&ours), props(e), "Burrow sends exactly these");
}

#[test]
fn a_put_body_and_an_answer_are_the_contracts() {
    let doc = contract();
    let put = &doc["paths"]["/items/{id}/syndication"]["put"];
    let req = &put["requestBody"]["content"]["application/json"]["schema"];
    assert_eq!(props(req), ["entries", "expectedRevision"]);
    assert_eq!(required(req), ["entries", "expectedRevision"]);
    assert_eq!(req["additionalProperties"], false);
    assert_eq!(
        req["properties"]["entries"]["maxItems"],
        syndication::MAX_ENTRIES
    );

    // Burrow's body: those keys, at most the cap, https only.
    let many: Vec<SyndicationEntry> = (1..=25)
        .map(|i| SyndicationEntry::new(format!("https://social.example.net/@burrow/{i}"), 1))
        .chain([SyndicationEntry::new("http://plain.example.net/1", 1)])
        .collect();
    let body = syndication::put_body(&many, 7);
    assert_eq!(keys(&body), props(req));
    assert_eq!(body["expectedRevision"], 7);
    let sent = body["entries"].as_array().unwrap();
    assert_eq!(sent.len(), syndication::MAX_ENTRIES);
    assert!(sent.iter().all(|e| keys(e) == ["url", "version"]));

    // The answer, `Syndication`: the list and the item's revision.
    let s = schema(&doc, "Syndication");
    assert_eq!(props(s), ["entries", "revision"]);
    assert_eq!(required(s), ["entries", "revision"]);
    let a = syndication::parse_answer(&json!({
        "entries": [{ "url": "https://social.example.net/@burrow/1", "version": 1 }],
        "revision": 3,
    }))
    .unwrap();
    assert_eq!(
        a.entries,
        [SyndicationEntry::new(
            "https://social.example.net/@burrow/1",
            1
        )]
    );
    assert_eq!(a.revision, 3);
}

/// Studio 0.67.1: an imported row carries its origin's `syndication`
/// (https entries only), on `ImportedItem`, on `ImportedReadingEntry`
/// (required, null when none) and on `ReadingEntry.imported`.
#[test]
fn imported_rows_carry_the_member_null_when_none() {
    let doc = contract();
    let entry_ref = json!({ "$ref": "#/components/schemas/SyndicationEntry" });
    let imported = &schema(&doc, "ReadingEntry")["properties"]["imported"];
    for (name, s) in [
        ("ImportedItem", schema(&doc, "ImportedItem")),
        ("ImportedReadingEntry", schema(&doc, "ImportedReadingEntry")),
        ("ReadingEntry.imported", imported),
    ] {
        let m = &s["properties"][syndication::MEMBER];
        assert_eq!(m["items"], entry_ref, "{name}.syndication");
    }
    let ire = schema(&doc, "ImportedReadingEntry");
    assert!(required(ire).contains(&"syndication".to_string()));
    assert_eq!(
        ire["properties"]["syndication"]["type"],
        json!(["array", "null"])
    );

    // How Burrow reads a served row's member: null is none, an array the
    // list with anything that isn't an https entry left out.
    assert_eq!(syndication::parse_member(Some(&Value::Null)), None);
    assert_eq!(
        syndication::parse_member(Some(&json!([
            { "url": "https://social.example.net/@kit/113", "version": 1 },
            { "url": "http://plain.example.net/1", "version": 1 },
            { "url": "https://social.example.net/@kit/114" },
        ]))),
        Some(vec![SyndicationEntry::new(
            "https://social.example.net/@kit/113",
            1
        )])
    );
}
