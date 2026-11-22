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

/// Runs `--run <base> <tip>` with a stand-in `cargo` first on PATH (a real
/// build never runs in a test) that logs each call to `calls` and reports one
/// failing test. Returns the exit status, the output, and the logged calls.
fn run_with_stub_cargo(r: &Repo, base: &str, tip: &str) -> (i32, String, String) {
    use std::os::unix::fs::PermissionsExt;
    let bin = tempfile::tempdir().unwrap();
    let cargo = bin.path().join("cargo");
    let fake = "#!/bin/sh\necho \"$@\" >>\"$(dirname \"$0\")/calls\"\necho 'test adds ... FAILED'\n";
    std::fs::write(&cargo, fake).unwrap();
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        bin.path().display(),
        std::env::var("PATH").unwrap()
    );
    let out = Command::new("bash")
        .arg(script())
        .args(["--run", base, tip])
        .current_dir(r.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("PATH", path)
        .env("CARGO_TARGET_DIR", bin.path().join("shared"))
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let calls = std::fs::read_to_string(bin.path().join("calls")).unwrap_or_default();
    (out.status.code().unwrap(), text, calls)
}

const FIXTURE: &str = "crates/x/tests/fixtures/data.toml";

/// A repository with the crate manifest in its base commit; returns it and
/// the base.
fn repo_with_crate() -> (Repo, String) {
    let r = Repo::new();
    r.write("crates/x/Cargo.toml", "[package]\nname = \"x\"\n");
    let base = r.commit("crate", None);
    (r, base)
}

#[test]
fn run_skips_a_test_commit_that_adds_no_test_function() {
    let (r, base) = repo_with_crate();
    r.write(FIXTURE, "a = 1\n");
    let fixture = r.commit("fixture", None);
    r.write(TEST, "#[test]\nfn adds() {}\n");
    let red = r.commit("red: add", None);
    r.write(CODE, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n");
    let tip = r.commit("green: add", None);
    let (code, out, calls) = run_with_stub_cargo(&r, &base, &tip);
    let short = |c: &str| r.git(&["rev-parse", "--short", c]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(&format!(
            "test-first: {} changes test files but adds no #[test]; skipped\n",
            short(&fixture)
        )),
        "{out}"
    );
    assert!(
        out.contains(&format!("x at {}: adds", short(&red))),
        "{out}"
    );
    assert!(out.contains("red before green at"), "{out}");
    assert_eq!(calls.lines().count(), 1, "{calls}");
}

#[test]
fn run_without_any_test_function_on_the_branch_says_so_and_exits_1() {
    let (r, base) = repo_with_crate();
    r.write(FIXTURE, "a = 1\n");
    r.commit("fixture", None);
    r.write("crates/x/tests/fixtures/more.toml", "b = 2\n");
    r.commit("another fixture", None);
    r.write(CODE, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n");
    let tip = r.commit("code", None);
    let (code, out, calls) = run_with_stub_cargo(&r, &base, &tip);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("no test commit on") && out.contains("adds a #[test] function"),
        "{out}"
    );
    assert!(!out.contains("adds no #[test] functions to run"), "{out}");
    assert!(calls.is_empty(), "{calls}");
}

#[test]
fn run_fails_when_the_first_commit_with_tests_comes_after_product_code() {
    let (r, base) = repo_with_crate();
    r.write(FIXTURE, "a = 1\n");
    r.commit("fixture", None);
    r.write(CODE, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n");
    let product = r.commit("code", None);
    r.write(TEST, "#[test]\nfn adds() {}\n");
    let tip = r.commit("tests", None);
    // The order check is unchanged: a test change (the fixture) came first.
    assert!(r.run(&[&base, &tip]).0);
    let (code, out, calls) = run_with_stub_cargo(&r, &base, &tip);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains(&format!(
            "comes after product code at {}",
            r.git(&["rev-parse", "--short", &product])
        )) && out.contains("can't be red before the feature"),
        "{out}"
    );
    assert!(calls.is_empty(), "{calls}");
}
