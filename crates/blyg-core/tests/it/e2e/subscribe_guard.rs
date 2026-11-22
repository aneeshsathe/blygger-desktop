//! Every subscription an e2e test makes is dropped by a guard (`Unsub`, or
//! `ApiUnsub` for one made through the owner API) bound right after it. All
//! tests share blyg `a` and subscribe to the same blyg `b`, so a test that
//! panics before its own `unsubscribe` left the subscription behind, and
//! every later test's subscribe failed with 409 "already subscribed" (twelve
//! knock-on failures from one in the studio 0.68 run).
//!
//! This test needs no studio: it reads the e2e sources and runs in the
//! normal `cargo test`.

use std::path::Path;

/// How many lines after a `subscribe(` call its guard may start.
const WITHIN: usize = 3;

#[test]
fn every_e2e_subscription_is_dropped_by_a_guard() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/it/e2e");
    let mut bad = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("tests/it/e2e") {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|x| x != "rs") || path.ends_with("subscribe_guard.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains(".subscribe(") || line.trim_start().starts_with("//") {
                continue;
            }
            let guarded = lines
                .iter()
                .skip(i + 1)
                .take(WITHIN)
                .any(|l| l.contains("Unsub("));
            if !guarded {
                let name = path.file_name().unwrap().to_string_lossy();
                bad.push(format!("{name}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "bind `let _unsub = Unsub(&me, sub.id.clone());` right after each subscribe, \
         so a failing test can't leave the next one \"already subscribed\":\n{}",
        bad.join("\n")
    );
}
