//! `scripts/test-first.sh` (CI's `test-first` job runs its `--pr` mode) on
//! small throwaway repositories: tests before code passes, code before tests
//! fails, and a `No-test:` trailer lets a change through.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/test-first.sh")
}

struct Repo(tempfile::TempDir);

impl Repo {
    fn new() -> Repo {
        let r = Repo(tempfile::tempdir().unwrap());
        r.git(&["init", "-q"]);
        r.write("README", "x\n");
        r.commit("start", None);
        r
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(self.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn write(&self, file: &str, text: &str) {
        let p = self.path().join(file);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    /// Commits everything; returns the commit id.
    fn commit(&self, subject: &str, no_test: Option<&str>) -> String {
        self.git(&["add", "-A"]);
        let msg = match no_test {
            Some(r) => format!("{subject}\n\nNo-test: {r}\n"),
            None => subject.to_string(),
        };
        self.git(&["commit", "-q", "-m", &msg]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn run(&self, args: &[&str]) -> (bool, String) {
        let out: Output = Command::new("bash")
            .arg(script())
            .args(args)
            .current_dir(self.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.success(), text)
    }
}

const CODE: &str = "crates/x/src/lib.rs";
const TEST: &str = "crates/x/tests/it.rs";

#[test]
fn tests_first_pass() {
    let r = Repo::new();
    let base = r.git(&["rev-parse", "HEAD"]);
    r.write(
        TEST,
        "#[test]\nfn adds() { assert_eq!(x::add(1, 1), 2); }\n",
    );
    r.commit("red: add", None);
    r.write(CODE, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n");
    let tip = r.commit("green: add", None);
    let (ok, out) = r.run(&[&base, &tip]);
    assert!(ok, "{out}");
    let (ok, out) = r.run(&["--pr", &base, &tip]);
    assert!(ok, "{out}");
}

#[test]
fn code_before_tests_fails_the_order_but_not_the_pr_check() {
    let r = Repo::new();
    let base = r.git(&["rev-parse", "HEAD"]);
    r.write(CODE, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n");
    let code = r.commit("add", None);
    r.write(TEST, "#[test]\nfn adds() {}\n");
    let tip = r.commit("tests for add", None);
    let (ok, out) = r.run(&[&base, &tip]);
    assert!(!ok, "{out}");
    assert!(
        out.contains(&format!("FAIL  code       {}", &code[..7])),
        "{out}"
    );
    assert!(out.contains(CODE), "{out}");
    // CI only asks that the PR changes tests at all.
    let (ok, out) = r.run(&["--pr", &base, &tip]);
    assert!(ok, "{out}");
}

#[test]
fn code_without_tests_fails_the_pr_check_unless_a_commit_says_why() {
    let r = Repo::new();
    let base = r.git(&["rev-parse", "HEAD"]);
    r.write(CODE, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n");
    let tip = r.commit("add", None);
    let (ok, out) = r.run(&["--pr", &base, &tip]);
    assert!(!ok, "{out}");
    assert!(out.contains(CODE) && out.contains("No-test:"), "{out}");

    r.write(
        CODE,
        "/// Adds.\npub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    );
    let tip = r.commit("doc comment", Some("a comment; nothing to test"));
    let (ok, out) = r.run(&["--pr", &base, &tip]);
    assert!(ok, "{out}");
    // The order check lets the trailered commit through, not the first one.
    let (ok, out) = r.run(&[&base, &tip]);
    assert!(!ok && out.contains("no-test"), "{out}");
}

#[test]
fn an_inline_test_module_and_test_files_count_as_tests() {
    let r = Repo::new();
    let base = r.git(&["rev-parse", "HEAD"]);
    r.write(CODE, "pub fn one() -> i32 { 1 }\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn one() { assert_eq!(super::one(), 1); }\n}\n");
    let tip = r.commit("one, with its test", None);
    assert!(r.run(&[&base, &tip]).0);
    assert!(r.run(&["--pr", &base, &tip]).0);

    // A test file, declared from product code: the declaration alone is a
    // test change, so the red commit holds no product code.
    let r = Repo::new();
    r.write("crates/x/src/walk.rs", "pub fn walk() {}\n");
    let base = r.commit("walk", None);
    r.write("crates/x/src/walk_tests.rs", "fn t() {}\n");
    r.write(
        "crates/x/src/walk.rs",
        "pub fn walk() {}\n\n#[cfg(test)]\nmod walk_tests;\n",
    );
    r.commit("red", None);
    r.write(
        "crates/x/src/walk.rs",
        "pub fn walk() { let _ = 1; }\n\n#[cfg(test)]\nmod walk_tests;\n",
    );
    let tip = r.commit("green", None);
    let (ok, out) = r.run(&[&base, &tip]);
    assert!(ok && out.contains("ok    tests      "), "{out}");
}

#[test]
fn run_wants_the_red_commit_to_hold_tests_only() {
    let r = Repo::new();
    let base = r.git(&["rev-parse", "HEAD"]);
    r.write(TEST, "#[test]\nfn adds() {}\n");
    r.write(CODE, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n");
    let tip = r.commit("add, with its test", None);
    // The order holds (same commit), but nothing can be red before it.
    assert!(r.run(&[&base, &tip]).0);
    let (ok, out) = r.run(&["--run", &base, &tip]);
    assert!(
        !ok && out.contains("can't be") && out.contains(CODE),
        "{out}"
    );
}

/// `--run` builds an old commit. A stand-in `cargo` records where it was told
/// to build: under the caller's target directory, never in it (a test binary
/// an older commit leaves there is one the next run takes for its own).
#[test]
fn run_builds_the_old_commit_in_a_target_dir_of_its_own() {
    use std::os::unix::fs::PermissionsExt;
    let r = Repo::new();
    let base = r.git(&["rev-parse", "HEAD"]);
    r.write("crates/x/Cargo.toml", "[package]\nname = \"x\"\n");
    r.write(TEST, "#[test]\nfn adds() {}\n");
    let tip = r.commit("red: add", None);
    let bin = tempfile::tempdir().unwrap();
    let cargo = bin.path().join("cargo");
    let fake = "#!/bin/sh\nprintf %s \"$CARGO_TARGET_DIR\" >\"$(dirname \"$0\")/built-in\"\necho 'test adds ... FAILED'\n";
    std::fs::write(&cargo, fake).unwrap();
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        bin.path().display(),
        std::env::var("PATH").unwrap()
    );
    let shared = bin.path().join("shared");
    let out = Command::new("bash")
        .arg(script())
        .args(["--run", &base, &tip])
        .current_dir(r.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("PATH", path)
        .env("CARGO_TARGET_DIR", &shared)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && text.contains("red: test adds"),
        "{text}"
    );
    let built_in = std::fs::read_to_string(bin.path().join("built-in")).unwrap();
    assert_eq!(Path::new(&built_in), shared.join("test-first"));
}
