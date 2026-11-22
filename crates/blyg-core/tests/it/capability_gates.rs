//! Features gated on the operations a blyg's capabilities list, never on
//! its `contractVersion` (skill `capability-gating`): a contract Burrow
//! can't read ("next") that lists them gets them, and one whose number
//! says yes but whose list doesn't, doesn't.

use crate::common::*;
use blyg_core::api::Api;

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
    api.put_changelog_settings(&s).expect("saved");

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

/// Every operation a gate looks for is one the newest contract names: a
/// typo would hide the feature on every studio.
#[test]
fn the_gated_operations_are_the_contracts() {
    let known = crate::common::contract::current().operation_ids();
    let gated = [
        blyg_core::contacts::CONTACTS_OPERATION,
        blyg_core::changelog::CHANGELOG_OPERATION,
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
