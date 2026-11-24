//! Features gated on the operations a blyg's capabilities list, never on
//! its `contractVersion` (skill `capability-gating`): a contract Burrow
//! can't read ("next") that lists them gets them, and one whose number
//! says yes but whose list doesn't, doesn't.
//!
//! The studio lists an operation only to a credential whose scopes grant
//! it. So a gate on something Burrow reads or shows names an operation a
//! read-scope token is listed: changelog display once followed
//! `summarizeChangelog` (draft scope), and vanished for a token that
//! couldn't draft. [`GATES`] says what each gate guards, and the scans
//! below keep that list whole: every operation constant in product code is
//! in it, and every gate asks for one of those constants.

use std::collections::{BTreeMap, BTreeSet};

use crate::common::contract::Contract;
use crate::common::*;
use crate::timing_scan::{close_of, is_ident, line_of, statement_end};
use crate::version_gates::{
    Source, enclosing, functions, ident_at, product_sources, skip_space, string_literals, top_level,
};
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

fn lens_reads(env: &Env) -> usize {
    env.mock.state().count("GET /api/reading?background")
}

/// A studio before 0.61 that ignores the filter answers its plain reading
/// list. That's judged on the first page, and remembered for that studio's
/// contract: later sessions don't ask again until the contract changes.
#[test]
fn a_studio_that_ignores_the_lens_filter_is_asked_once_per_contract() {
    let plain = serde_json::json!({
        "imported": { "subscriptionId": "S1", "remoteId": "R1" }
    });
    let env = env_at("6.1.0", &[]);
    env.mock.state().background_lens = Some(vec![plain]);
    let opts = || SyncOptions {
        start_worker: false,
        ..fast()
    };
    let b = env.open(opts());
    b.sync_now().unwrap();
    assert!(!b.background_served(), "plain rows aren't the lens");
    assert_eq!(lens_reads(&env), 1, "one page judged");
    drop(b);

    let b = env.open(opts());
    b.sync_now().unwrap();
    assert!(!b.background_served());
    assert_eq!(lens_reads(&env), 1, "not asked again for this contract");
    drop(b);

    env.mock.state().contract_version = Some("6.2.0".into());
    let b = env.open(opts());
    b.sync_now().unwrap();
    assert_eq!(lens_reads(&env), 2, "a new contract is asked again");
}

/// On a contract Burrow doesn't know, plain rows may be a renamed field,
/// not an old studio: "no lens" isn't remembered there, and the next
/// session asks again.
#[test]
fn a_newer_contracts_plain_rows_arent_remembered() {
    let plain = serde_json::json!({
        "imported": { "subscriptionId": "S1", "remoteId": "R1" }
    });
    let env = env_at("7.0.0", &[]);
    env.mock.state().background_lens = Some(vec![plain]);
    let opts = || SyncOptions {
        start_worker: false,
        ..fast()
    };
    let b = env.open(opts());
    b.sync_now().unwrap();
    assert!(!b.background_served());
    drop(b);
    let b = env.open(opts());
    b.sync_now().unwrap();
    assert_eq!(lens_reads(&env), 2, "asked again");
}

/// An empty answer is a lens with nothing in it: a studio with Background
/// and nothing there yet (the common case on a new blyg) keeps Background.
/// An older studio that ignored the filter answers empty only when it holds
/// no reading posts, where nothing can be shown wrongly.
#[test]
fn an_empty_lens_is_a_lens() {
    let env = env_at("6.1.0", &[]);
    env.mock.state().background_lens = Some(vec![]);
    let b = env.open(SyncOptions {
        start_worker: false,
        ..fast()
    });
    b.sync_now().unwrap();
    assert!(b.background_served());
}

/// Whether the blyg has changelog display is read once a pull, not on each
/// call (`Api::new_pull` forgets it).
#[test]
fn changelog_display_is_read_once_a_pull() {
    let env = env_at("6.1.0", &[]);
    env.mock.state().settings = Some(serde_json::json!({ "changelogDisplay": "history" }));
    env.mock.state().unchecked.push("/api/settings".into());
    let api = Api::new(&env.mock.url, TOKEN);
    let reads = || env.mock.state().count("GET /api/settings");
    assert!(api.serves_changelog().unwrap());
    assert!(api.serves_changelog().unwrap());
    assert_eq!(reads(), 1);
    api.new_pull();
    assert!(api.serves_changelog().unwrap());
    assert_eq!(reads(), 2);
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
    for (constant, ops) in operation_constants(&product_sources()) {
        assert!(
            !ops.is_empty(),
            "crates/{constant} names no operation: a gate constant is the operationId (a string \
             literal, or a slice of them) the capabilities are asked for. Give it one, or \
             rename it without OPERATION if it isn't a gate."
        );
        for op in ops {
            assert!(
                known.contains(&op),
                "crates/{constant} asks for \"{op}\", which is no operationId in \
                 crates/blyg-core/tests/fixtures/openapi-current.json: no studio lists it, so \
                 the feature is hidden everywhere. Use the contract's spelling."
            );
        }
    }
}

/// What a gate guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Guards {
    /// Something Burrow reads or shows: a token with only the read scope
    /// must still get it.
    Read,
    /// Something Burrow sends: the operation is the write itself.
    Write,
    /// A request Burrow makes only where an older contract lists it (the
    /// current one has no such operation): the gate keeps Burrow from
    /// asking a blyg that would answer 404.
    Retired,
}

/// One operation constant a gate asks the capabilities for.
struct Gate {
    /// `<file under crates/>::<NAME>`.
    constant: &'static str,
    guards: Guards,
    what: &'static str,
}

/// Every capability gate Burrow has. `every_operation_constant_is_classified`
/// fails on a constant in product code that isn't here.
const GATES: &[Gate] = &[
    // --- Burrow in a browser (blyg-web/src/parity/mod.rs `offered`) ---
    Gate {
        constant: "blyg-web/src/parity/mod.rs::MARK_BATCH_OPERATION",
        guards: Guards::Write,
        what: "the web's Mark all read, one request for the chosen source",
    },
    Gate {
        constant: "blyg-web/src/parity/mod.rs::STALE_OPERATION",
        guards: Guards::Read,
        what: "the web's Updates screen: threads whose quotes have newer versions",
    },
    Gate {
        constant: "blyg-web/src/parity/mod.rs::REFRESH_OPERATION",
        guards: Guards::Write,
        what: "the web's Refresh of a thread's quotes",
    },
    Gate {
        constant: "blyg-web/src/parity/mod.rs::CHANGES_OPERATION",
        guards: Guards::Retired,
        what: "the web's `GET /api/changes`, asked only where the capabilities list it",
    },
    Gate {
        constant: "blyg-web/src/parity/mod.rs::FRESHNESS_OPERATION",
        guards: Guards::Read,
        what: "the web's probe of one thread's quotes at their blygs",
    },
    // --- studio 0.69 ---
    Gate {
        constant: "blyg-core/src/agentic.rs::GET_OPERATION",
        guards: Guards::Read,
        what: "reading and showing the Agentic feed's edition",
    },
    Gate {
        constant: "blyg-core/src/agentic.rs::LIST_OPERATION",
        guards: Guards::Read,
        what: "the Agentic feed list and the tab's name",
    },
    Gate {
        constant: "blyg-core/src/dictionary.rs::GET_OPERATION",
        guards: Guards::Read,
        what: "the blyg's dictionary, which the spellcheck accepts",
    },
    Gate {
        constant: "blyg-core/src/dictionary.rs::PUT_OPERATION",
        guards: Guards::Write,
        what: "sending a word to (or taking one out of) the blyg's dictionary",
    },
    Gate {
        constant: "blyg-core/src/helpers.rs::SUMMARIZE_OPERATION",
        guards: Guards::Write,
        what: "asking the blyg to write Catch up's summary (Burrow's own AI writes it otherwise)",
    },
    Gate {
        constant: "blyg-core/src/api/read_batch.rs::READ_BATCH_OPERATION",
        guards: Guards::Read,
        what: "reading many items in one request (a POST, on the read scope)",
    },
    Gate {
        constant: "blyg-core/src/background.rs::OPERATION",
        guards: Guards::Write,
        what: "placing a post or a blyg in Background (`background::places`)",
    },
    Gate {
        constant: "blyg-core/src/background.rs::LENS_OPERATION",
        guards: Guards::Read,
        what: "reading and showing the Background lens (`background::serves`)",
    },
    Gate {
        constant: "blyg-core/src/changelog.rs::CHANGELOG_OPERATION",
        guards: Guards::Write,
        what: "asking the blyg to write a changelog summary",
    },
    Gate {
        constant: "blyg-core/src/changelog.rs::SETTINGS_OPERATION",
        guards: Guards::Read,
        what: "the changelog display settings, as a read of the settings shows them",
    },
    Gate {
        constant: "blyg-core/src/contacts.rs::CONTACTS_OPERATION",
        guards: Guards::Read,
        what: "the address book, the `@handle` rewrite and the mentions lens",
    },
    Gate {
        constant: "blyg-core/src/outbound.rs::GROUPS_OPERATIONS",
        guards: Guards::Read,
        what: "sent mentions in groups, and their summary",
    },
    Gate {
        constant: "blyg-core/src/shelf.rs::GET_OPERATION",
        guards: Guards::Read,
        what: "reading and showing the blyg's shelf (`shelf::serves`)",
    },
    Gate {
        constant: "blyg-core/src/shelf.rs::PUT_OPERATION",
        guards: Guards::Write,
        what: "sending a change to the shelf (`shelf::takes`)",
    },
    Gate {
        constant: "blyg-core/src/syndication.rs::PUT_OPERATION",
        guards: Guards::Write,
        what: "sending a post's copies-elsewhere list",
    },
];

/// The scope every token that can read the blyg has.
const READ_SCOPE: &str = "owner:read";

/// The operations among `ops` that `contract` doesn't grant to a token
/// with only the read scope: ones needing another scope, ones only the
/// owner's session may call, and ones it doesn't have.
fn beyond_read(ops: &[String], contract: &Contract) -> Vec<String> {
    ops.iter()
        .filter(|op| {
            contract
                .oauth_scopes(op.as_str())
                .is_none_or(|scopes| scopes.iter().any(|s| s.as_str() != READ_SCOPE))
        })
        .cloned()
        .collect()
}

/// Each operation constant in product code (a `const` with `OPERATION` in
/// its name), as `<file>::<NAME>`, with the operations it names.
fn operation_constants(sources: &[Source]) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for s in sources {
        let b = s.code.as_bytes();
        for (at, _) in s.code.match_indices("const ") {
            if at > 0 && is_ident(b[at - 1]) {
                continue;
            }
            let name = ident_at(&s.code, skip_space(&s.code, at + 6));
            if !name.contains("OPERATION") {
                continue;
            }
            let end = statement_end(b, at + 6);
            let ops: Vec<String> = s.src[at..end]
                .split('"')
                .skip(1)
                .step_by(2)
                .map(str::to_string)
                .collect();
            out.insert(format!("{}::{name}", s.path), ops);
        }
    }
    out
}

/// Each call of `api::offers` or `Api::offers_all` in `s` whose operations
/// aren't all constants named in `constants`: `(line, function, the call)`.
fn unnamed_asks(s: &Source, constants: &BTreeSet<&str>) -> Vec<(usize, String, String)> {
    let b = s.code.as_bytes();
    let funcs = functions(&s.code);
    let mut out = vec![];
    for call in ["offers(", "offers_all("] {
        for (at, _) in s.code.match_indices(call) {
            let before = if at > 0 { b[at - 1] } else { b' ' };
            // `.offers(` is a method of another type (background rules);
            // `api::offers` is a function.
            if is_ident(before)
                || (call == "offers(" && before == b'.')
                || s.code[..at].trim_end().ends_with("fn")
            {
                continue;
            }
            let open = at + call.len() - 1;
            let close = close_of(b, open);
            let args = top_level(&s.code[open + 1..(close - 1).max(open + 1)]);
            // A literal among them (blank in `code`) is no constant.
            let literal = s.src[at..close].contains('"');
            let named = !literal
                && args.last().is_some_and(|arg| {
                    let list = arg.trim_start_matches('&').trim();
                    let list = list
                        .strip_prefix('[')
                        .and_then(|l| l.strip_suffix(']'))
                        .unwrap_or(list);
                    let names = top_level(list);
                    !names.is_empty()
                        && names
                            .iter()
                            .all(|&n| constants.contains(n.rsplit("::").next().unwrap_or(n)))
                });
            if !named {
                let call: Vec<&str> = s.src[at..close].split_whitespace().collect();
                out.push((
                    line_of(&s.code, at),
                    enclosing(&funcs, at).to_string(),
                    call.join(" "),
                ));
            }
        }
    }
    out
}

/// Calls of `offers`/`offers_all` that pass on operations they were handed:
/// `(file, function, why)`.
const PASSES_ON: &[(&str, &str, &str)] = &[(
    "blyg-core/src/api/mod.rs",
    "offers_all",
    "asks `offers` for each operation its caller named",
)];

/// The functions that read the capabilities' `operations` list themselves:
/// `(file, function, why)`. A new gate calls `api::offers`.
const READS_THE_LIST: &[(&str, &str, &str)] = &[
    (
        "blyg-core/src/api/mod.rs",
        "offers",
        "the one reading of the list",
    ),
    (
        "blyg-core/src/api/read_batch.rs",
        "serves_read_batch",
        "its own copy of `offers`, from before there was one; asks for READ_BATCH_OPERATION",
    ),
    (
        "blyg-core/src/syndication.rs",
        "serves",
        "its own copy of `offers`, from before there was one; asks for PUT_OPERATION",
    ),
];

/// `found` (each `(file, function, what was found)`) against `allowed`:
/// what isn't allowed, then the allowed entries nothing matched.
fn against(
    found: &[(String, String, String)],
    allowed: &[(&str, &str, &str)],
) -> (Vec<String>, Vec<String>) {
    let loose = found
        .iter()
        .filter(|f| !allowed.iter().any(|a| a.0 == f.0 && a.1 == f.1))
        .map(|f| format!("crates/{}: fn {}: {}", f.0, f.1, f.2))
        .collect();
    let stale = allowed
        .iter()
        .filter(|a| !found.iter().any(|f| a.0 == f.0 && a.1 == f.1))
        .map(|a| format!("{} fn {} ({})", a.0, a.1, a.2))
        .collect();
    (loose, stale)
}

#[test]
fn every_operation_constant_is_classified() {
    let constants = operation_constants(&product_sources());
    let listed: BTreeSet<&str> = GATES.iter().map(|g| g.constant).collect();
    assert_eq!(
        listed.len(),
        GATES.len(),
        "GATES (capability_gates.rs) lists a constant twice: keep one row for it"
    );
    let unclassified: Vec<String> = constants
        .iter()
        .filter(|(constant, _)| !listed.contains(constant.as_str()))
        .map(|(constant, ops)| format!("{constant} ({})", ops.join(", ")))
        .collect();
    assert!(
        unclassified.is_empty(),
        "An operation constant in product code has no row in GATES (capability_gates.rs), so\n\
         nothing checks that a read-only sign-in still gets what it guards. Add a row for it:\n\
         Guards::Read if anything Burrow reads or shows depends on the gate (its operation\n\
         must then need only the read scope), Guards::Write if only a write does. A feature\n\
         with both gets two constants, one each:\n{}",
        unclassified
            .iter()
            .map(|c| format!("crates/{c}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let gone: Vec<&str> = listed
        .iter()
        .copied()
        .filter(|constant| !constants.contains_key(*constant))
        .collect();
    assert!(
        gone.is_empty(),
        "GATES (capability_gates.rs) has rows for constants product code no longer has. \
         Remove the rows (or fix the path if the constant moved): {gone:?}"
    );
}

#[test]
fn every_gate_asks_for_an_operation_constant() {
    let sources = product_sources();
    let constants = operation_constants(&sources);
    let names: BTreeSet<&str> = constants
        .keys()
        .map(|k| k.rsplit("::").next().unwrap_or(k.as_str()))
        .collect();
    let mut asks = vec![];
    let mut reads = vec![];
    for s in &sources {
        for (line, func, call) in unnamed_asks(s, &names) {
            asks.push((s.path.clone(), func, format!("line {line}: {call}")));
        }
        let funcs = functions(&s.code);
        for at in string_literals(&s.src, "\"operations\"") {
            let line = line_of(&s.src, at);
            let func = enclosing(&funcs, at).to_string();
            reads.push((s.path.clone(), func, format!("line {line}")));
        }
    }
    let (loose, stale) = against(&asks, PASSES_ON);
    assert!(
        loose.is_empty(),
        "A call of api::offers or Api::offers_all asks for something other than an operation\n\
         constant (a string literal, a variable, or a const without OPERATION in its name),\n\
         so the gate is in no list and nothing checks its scope. Put the operationId in a\n\
         `pub const …_OPERATION: &str` beside the feature, pass that, and add its row to\n\
         GATES (capability_gates.rs):\n{}",
        loose.join("\n")
    );
    assert!(
        stale.is_empty(),
        "PASSES_ON (capability_gates.rs) names functions that no longer call offers with \
         operations they were handed. Remove the entries: {stale:?}"
    );
    let (loose, stale) = against(&reads, READS_THE_LIST);
    assert!(
        loose.is_empty(),
        "A function reads the capabilities' \"operations\" list itself. A second reading of\n\
         the list is a gate the scans above can't see. Replace it with\n\
         blyg_core::api::offers(capabilities, YOUR_OPERATION), or Api::offers_all(&[…]) when\n\
         the capabilities are still to be fetched:\n{}",
        loose.join("\n")
    );
    assert!(
        stale.is_empty(),
        "READS_THE_LIST (capability_gates.rs) names functions that no longer read the \
         \"operations\" list. Remove the entries: {stale:?}"
    );
}

/// A read or display gate names only operations the contract grants on the
/// read scope (`security`'s `ownerOAuth` scopes in `openapi-current.json`),
/// so the studio lists them to every token that can read the blyg.
#[test]
fn read_gates_ask_for_operations_a_read_token_is_listed() {
    let constants = operation_constants(&product_sources());
    let contract = crate::common::contract::current();
    let mut beyond = vec![];
    for g in GATES.iter().filter(|g| g.guards == Guards::Read) {
        let ops = constants.get(g.constant).cloned().unwrap_or_default();
        for op in beyond_read(&ops, contract) {
            let needs = contract.oauth_scopes(&op).map_or_else(
                || "the owner's session".to_string(),
                |scopes| scopes.join(", "),
            );
            beyond.push((g.constant, op, needs, g.what));
        }
    }
    let beyond: Vec<String> = beyond
        .iter()
        .map(|(constant, op, needs, what)| {
            format!("crates/{constant}: {op} needs {needs}\n    it guards: {what}")
        })
        .collect();
    assert!(
        beyond.is_empty(),
        "A constant that GATES (capability_gates.rs) lists as guarding a read names an\n\
         operation the studio doesn't list to a token with only {READ_SCOPE}, so what it\n\
         guards vanishes for that token. In the product file named below, gate the read on\n\
         the operation that serves it (its own constant: `background::LENS_OPERATION` beside\n\
         `OPERATION`, `shelf::GET_OPERATION` beside `PUT_OPERATION`), and ask for this one\n\
         only before the write. Change a GATES row to Guards::Write only when nothing read\n\
         or shown depends on that gate:\n{}",
        beyond.join("\n")
    );
}

/// A retired gate names an operation the current contract doesn't have, so
/// it can't hide a read a current blyg serves.
#[test]
fn retired_gates_name_operations_the_current_contract_lacks() {
    let constants = operation_constants(&product_sources());
    let contract = crate::common::contract::current();
    let live: Vec<String> = GATES
        .iter()
        .filter(|g| g.guards == Guards::Retired)
        .flat_map(|g| constants.get(g.constant).cloned().unwrap_or_default())
        .filter(|op| contract.oauth_scopes(op).is_some())
        .collect();
    assert!(
        live.is_empty(),
        "GATES (capability_gates.rs) calls these operations retired, but the current contract has \
         them: classify each as Guards::Read or Guards::Write: {live:?}"
    );
}

/// The past instance: changelog display followed `summarizeChangelog`
/// (commit fe08ad5's `Api::serves_changelog`), a draft-scope operation.
#[test]
fn a_display_gated_on_a_draft_operation_is_caught() {
    let contract = crate::common::contract::current();
    let ops = |names: &[&str]| -> Vec<String> { names.iter().map(|n| n.to_string()).collect() };
    assert_eq!(
        beyond_read(&ops(&["summarizeChangelog"]), contract),
        ["summarizeChangelog"]
    );
    // What it follows now, and a read that is a POST.
    assert!(beyond_read(&ops(&["getSettings", "batchRead"]), contract).is_empty());
    // An operation the contract doesn't have is listed to no one.
    assert_eq!(
        beyond_read(&ops(&["noSuchOperation"]), contract),
        ["noSuchOperation"]
    );
}

#[test]
fn the_gate_scan_reads_constants_and_calls() {
    let src = r#"
pub const GET_OPERATION: &str = "getShelf";
pub const BOTH_OPERATIONS: &[&str] = &["listMentionGroups", "getMentionSummary"];
pub const SERVED_META: &str = "shelf_served";

pub fn serves(capabilities: &Value) -> bool {
    crate::api::offers(capabilities, GET_OPERATION) && rules.offers(rule)
}

impl Api {
    pub fn offers_all(&self, ops: &[&str]) -> Result<bool> {
        Ok(ops.iter().all(|op| offers(&v, op)))
    }
    pub fn a(&self) -> Result<bool> {
        self.offers_all(crate::outbound::BOTH_OPERATIONS)
    }
    pub fn b(&self) -> Result<bool> {
        self.offers_all(&[GET_OPERATION, "putShelf"])
    }
    pub fn c(&self, caps: &Value) -> bool {
        offers(caps, "listContacts") || offers(caps, SERVED_META)
    }
}
"#;
    let source = Source {
        path: "k/src/shelf.rs".into(),
        src: src.into(),
        code: crate::version_gates::product_code(src),
    };
    let constants = operation_constants(std::slice::from_ref(&source));
    let keys: Vec<(&str, usize)> = constants
        .iter()
        .map(|(k, ops)| (k.as_str(), ops.len()))
        .collect();
    assert_eq!(
        keys,
        [
            ("k/src/shelf.rs::BOTH_OPERATIONS", 2),
            ("k/src/shelf.rs::GET_OPERATION", 1),
        ]
    );
    let names = BTreeSet::from(["BOTH_OPERATIONS", "GET_OPERATION"]);
    let found: Vec<(usize, String)> = unnamed_asks(&source, &names)
        .into_iter()
        .map(|(line, func, _)| (line, func))
        .collect();
    let expected = [(12, "offers_all"), (21, "c"), (21, "c"), (18, "b")];
    assert_eq!(found.len(), expected.len(), "{found:?}");
    for (f, e) in found.iter().zip(expected) {
        assert_eq!((f.0, f.1.as_str()), e, "{found:?}");
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
