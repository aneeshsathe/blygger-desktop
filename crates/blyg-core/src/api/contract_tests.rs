//! What Burrow makes of a studio's `contractVersion` (`contract`), and
//! whether that studio's lists may remove what's held here.

use super::*;

#[test]
fn contracts_five_and_six_are_spoken_and_their_lists_prune() {
    for v in ["5.0.0", " 5.3.1 ", "6.0.0", "6.1.0"] {
        assert_eq!(contract(Some(v)), Contract::Supported, "{v}");
    }
    assert_eq!(Contract::Supported.prune(), Some(Prune::Allowed));
}

#[test]
fn an_older_contract_is_too_old_and_nothing_is_read() {
    assert_eq!(contract(Some("4.0.0")), Contract::TooOld);
    assert_eq!(contract(None), Contract::TooOld, "no capabilities");
    assert_eq!(Contract::TooOld.prune(), None);
}

/// Studio 0.67 moved to contract 6 and released Burrow stopped syncing.
/// A newer major syncs on, but its lists never remove what's held here.
#[test]
fn a_newer_major_syncs_without_pruning() {
    assert_eq!(contract(Some("7.0.0")), Contract::Newer("7.0.0".into()));
    assert_eq!(contract(Some(" 12.3.1 ")), Contract::Newer("12.3.1".into()));
    assert_eq!(
        Contract::Newer("7.0.0".into()).prune(),
        Some(Prune::Withheld)
    );
}

/// A feature is offered when the capabilities list its operation, as an
/// array of names or an object keyed by them (skill `capability-gating`).
#[test]
fn an_operation_is_offered_when_the_capabilities_list_it() {
    let caps = json!({ "contractVersion": "next", "operations": ["listContacts"] });
    assert!(offers(&caps, "listContacts"));
    assert!(!offers(&caps, "getShelf"));
    assert!(offers(
        &json!({ "operations": { "getShelf": {} } }),
        "getShelf"
    ));
    assert!(!offers(&json!({}), "getShelf"));
}

#[test]
fn a_version_that_isnt_a_number_is_newer() {
    assert_eq!(contract(Some("next")), Contract::Newer("next".into()));
    assert_eq!(contract(Some("v7")), Contract::Newer("v7".into()));
}

/// Only a 400 about `wholeBlyg` itself is answered by asking again without
/// it. The studio's strict body (0.49, contract 5.0) says, through its
/// `schemaError`: `request: Unrecognized key: "wholeBlyg"`, with the issue
/// in `details.issues`; the test mock says ``body: unknown field `wholeBlyg` ``.
#[test]
fn only_a_refusal_naming_whole_blyg_is_retried_without_it() {
    let rejected = |message: &str, details: &[&str]| CoreError::Rejected {
        status: 400,
        message: message.into(),
        details: details.iter().map(|d| d.to_string()).collect(),
    };
    for e in [
        rejected("request: Unrecognized key: \"wholeBlyg\"", &[]),
        rejected(
            "invalid input",
            &["request: Unrecognized key: \"wholeBlyg\""],
        ),
        rejected("body: unknown field `wholeBlyg`", &[]),
    ] {
        assert!(names_whole_blyg(&e), "{e:?}");
    }
    for e in [
        rejected("url: Invalid input: expected string", &[]),
        rejected("could not resolve this URL", &["tried 2 addresses"]),
        CoreError::Rejected {
            status: 422,
            message: "wholeBlyg needs a hopper".into(),
            details: vec![],
        },
        CoreError::NotFound,
    ] {
        assert!(!names_whole_blyg(&e), "{e:?}");
    }
}
