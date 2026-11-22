//! Features gated on the operations a blyg's capabilities list, never on
//! its `contractVersion` (skill `capability-gating`): a contract Burrow
//! can't read ("next") that lists them gets them, and one whose number
//! says yes but whose list doesn't, doesn't.

use crate::common::*;
use blyg_core::api::Api;
use blyg_core::*;

fn env_at(contract: &str, ops: &[&str]) -> Env {
    let env = Env::new();
    {
        let mut st = env.mock.state();
        st.contract_version = Some(contract.into());
        st.operations = ops.iter().map(|o| o.to_string()).collect();
    }
    env
}

/// Summaries follow `summarizeChangelog`, a draft-scope operation the
/// studio lists only to a token that may draft.
#[test]
fn changelog_summaries_follow_summarize_changelog() {
    let env = env_at("next", &["summarizeChangelog"]);
    assert!(
        Api::new(&env.mock.url, TOKEN)
            .serves_changelog_summary()
            .unwrap()
    );
    let env = env_at("6.1.0", &[]);
    assert!(
        !Api::new(&env.mock.url, TOKEN)
            .serves_changelog_summary()
            .unwrap()
    );
}

/// The changelog display keys (and the note draft's `history` flag) follow
/// what a read of the settings shows, so a token without the draft scope
/// still sees and keeps the blyg's display settings.
#[test]
fn changelog_display_follows_the_settings_a_read_shows() {
    let env = env_at("6.1.0", &[]);
    {
        let mut st = env.mock.state();
        st.settings = Some(serde_json::json!({
            "changelogDisplay": "history", "readerChangelogDisplay": "summary"
        }));
        st.unchecked.push("/api/settings".into());
    }
    let api = Api::new(&env.mock.url, TOKEN);
    assert!(!api.serves_changelog_summary().unwrap(), "no draft scope");
    assert!(api.serves_changelog().unwrap());
    let s = api.changelog_settings().unwrap().expect("display settings");
    // Sent (the mock's 5.0.0 schema then refuses the keys; a 0.58 studio keeps them).
    let _ = api.put_changelog_settings(&s);
    assert_eq!(env.mock.state().count("PATCH /api/settings"), 1, "sent");

    // A studio before 0.58: no keys, nothing served or sent.
    let env = env_at("5.0.0", &[]);
    let api = Api::new(&env.mock.url, TOKEN);
    assert!(!api.serves_changelog().unwrap());
    assert_eq!(api.changelog_settings().unwrap(), None);
    assert!(api.put_changelog_settings(&s).is_err());
    assert!(env.mock.state().settings_puts.is_empty());
}

#[test]
fn contacts_follow_list_contacts() {
    let env = env_at("7.0.0", &["listContacts"]);
    assert!(Api::new(&env.mock.url, TOKEN).serves_contacts().unwrap());
    let env = env_at("6.1.0", &[]);
    assert!(!Api::new(&env.mock.url, TOKEN).serves_contacts().unwrap());
}

#[test]
fn mention_groups_follow_their_operations() {
    let both = ["listMentionGroups", "getMentionSummary"];
    let env = env_at("next", &both);
    assert!(Api::new(&env.mock.url, TOKEN).serves_groups().unwrap());
    let env = env_at("6.1.0", &["listMentionGroups"]);
    assert!(!Api::new(&env.mock.url, TOKEN).serves_groups().unwrap());
}

/// Saved is read where `getShelf` is listed (owner:read) and written only
/// where `putShelf` is too (owner:manage): a read-only sign-in still reads it.
#[test]
fn saved_reads_follow_get_shelf_and_writes_put_shelf() {
    use blyg_core::shelf;
    let caps = |ops: &[&str]| serde_json::json!({ "operations": ops });
    assert!(shelf::serves(&caps(&["getShelf"])));
    assert!(!shelf::takes(&caps(&["getShelf"])));
    assert!(shelf::takes(&caps(&["getShelf", "putShelf"])));
    assert!(!shelf::serves(&caps(&["putShelf"])));

    let env = env_at("6.1.0", &["getShelf"]);
    let api = Api::new(&env.mock.url, TOKEN);
    assert!(api.serves_shelf().unwrap());
    assert!(!api.takes_shelf().unwrap());
}

/// The Background lens is read with `listReading` (owner:read, its
/// `background` filter); placing a post needs `setReadingPlacement`
/// (owner:manage).
#[test]
fn background_reads_follow_list_reading_and_placing_set_reading_placement() {
    let env = env_at("6.1.0", &[]);
    let api = Api::new(&env.mock.url, TOKEN);
    assert!(api.serves_background().unwrap(), "listReading is listed");
    assert!(!api.places_background().unwrap());
    let env = env_at("6.1.0", &["setReadingPlacement"]);
    assert!(Api::new(&env.mock.url, TOKEN).places_background().unwrap());
}

fn lens_entry(rid: &str) -> serde_json::Value {
    serde_json::json!({
        "imported": { "subscriptionId": "S1", "remoteId": rid },
        "background": { "reasons": ["minor"] }
    })
}

/// A read-only sign-in shows the blyg's Background, and offers no
/// placement (that write would be refused).
#[test]
fn a_read_only_sign_in_shows_background_but_places_nothing() {
    use blyg_core::background::{Key, Placement};
    let env = env_at("6.1.0", &[]);
    env.mock.state().background_lens = Some(vec![lens_entry("R1")]);
    let b = env.open(SyncOptions {
        start_worker: false,
        ..fast()
    });
    b.sync_now().unwrap();
    assert!(b.background_served());
    assert!(!b.background_placing());
    let set = b.background_set().expect("the lens was read");
    assert!(set.entries.iter().any(|(k, _)| k.1 == "R1"));
    let key: Key = ("S1".into(), "R1".into());
    assert!(
        b.set_reading_placement(&key, false, Some(Placement::Foreground))
            .is_err()
    );
    assert_eq!(env.mock.state().count("PUT /api/reading/placement"), 0);
}

/// A studio before Background refuses the lens's filter: no Background here.
#[test]
fn a_studio_without_the_lens_has_no_background() {
    let env = env_at("6.1.0", &[]);
    let b = env.open(SyncOptions {
        start_worker: false,
        ..fast()
    });
    b.sync_now().unwrap();
    assert!(!b.background_served());
    assert!(b.background_set().is_none());
}

/// `wholeBlyg` on `POST /api/subscriptions` has no operation of its own.
/// Only the floor contract (5.0) is known to refuse it; anything else is
/// sent it, and a studio whose strict body refuses it (400) is asked again
/// without it, and remembered.
#[test]
fn whole_blyg_is_tried_and_a_refusal_remembered() {
    // The mock's request schema is 5.0.0's: it refuses the key with 400.
    let env = env_at("next", &[]);
    env.mock.state().unchecked.push("/api/subscriptions".into());
    let api = Api::new(&env.mock.url, TOKEN);
    let p = api.preview_subscription("https://them.example/").unwrap();
    assert_eq!(p.title, "Their blyg");
    api.preview_subscription("https://them.example/").unwrap();
    let posts = env.mock.state().count("POST /api/subscriptions");
    assert_eq!(
        posts, 3,
        "with the key (400), without it, then without only"
    );

    // The floor contract is never sent it.
    let env = env_at("5.0.0", &[]);
    let api = Api::new(&env.mock.url, TOKEN);
    api.preview_subscription("https://them.example/").unwrap();
    assert_eq!(env.mock.state().count("POST /api/subscriptions"), 1);
}

/// Studio 0.69: the dictionary's read follows `getDictionary` (a read-only
/// sign-in still gets the accepted words), its write `putDictionarySource`;
/// the Agentic feed list follows `listAgenticFeeds`, an edition `getAgentic`.
#[test]
fn studio_069_reads_and_writes_follow_their_own_operations() {
    let env = env_at("next", &["getDictionary", "getAgentic"]);
    let api = Api::new(&env.mock.url, TOKEN);
    assert!(api.serves_dictionary().unwrap(), "read");
    assert!(!api.serves_dictionary_write().unwrap(), "no write");
    assert!(api.serves_agentic().unwrap());
    assert_eq!(api.agentic_feeds().unwrap(), None, "no listAgenticFeeds");
    let env = env_at("6.2.0", &[]);
    let api = Api::new(&env.mock.url, TOKEN);
    assert!(!api.serves_dictionary().unwrap(), "never by version");
    assert!(!api.serves_agentic().unwrap());
}

/// Every operation a gate looks for is one the newest contract names: a
/// typo would hide the feature on every studio.
#[test]
fn the_gated_operations_are_the_contracts() {
    let known = crate::common::contract::current().operation_ids();
    let gated = [
        blyg_core::contacts::CONTACTS_OPERATION,
        blyg_core::changelog::CHANGELOG_OPERATION,
        blyg_core::changelog::SETTINGS_OPERATION,
        // --- studio 0.69 --- reads: the edition, the feed list, the
        // dictionary, the summary; a write: the dictionary's words.
        blyg_core::agentic::GET_OPERATION,
        blyg_core::agentic::LIST_OPERATION,
        blyg_core::dictionary::GET_OPERATION,
        blyg_core::dictionary::PUT_OPERATION,
        blyg_core::helpers::SUMMARIZE_OPERATION,
    ]
    .into_iter()
    .chain(blyg_core::outbound::GROUPS_OPERATIONS.iter().copied());
    for op in gated {
        assert!(known.iter().any(|k| k == op), "{op} isn't in the contract");
    }
}

#[test]
fn only_the_floor_contract_is_known_to_refuse_whole_blyg() {
    use blyg_core::api::takes_whole_blyg;
    assert!(!takes_whole_blyg("5.0.0"));
    assert!(!takes_whole_blyg(" 5.0.3 "));
    for v in ["5.1.0", "6.0.0", "7.0.0", "next", "5"] {
        assert!(takes_whole_blyg(v), "{v}");
    }
}
