//! Shell that CI or a release runs must not use bash pattern substitution
//! (`${var//…}`), and the release-notes script must finish on a large
//! changelog section under `/bin/bash`.
//!
//! The mistake: `scripts/changelog-notes.sh` tested emptiness with
//! `${notes//[[:space:]]/}`. Under macOS's `/bin/bash` (3.2, which the
//! release runner uses) that substitution on a large string is extremely
//! slow: on 0.28.0's 60 KB section it ran 76 minutes and the release job
//! timed out. Bash 5 took about a second, so dev and CI never saw it.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Whether `line` contains a `${name/…}` form: `${`, an identifier, an
/// optional `[subscript]`, then `/` (so `//`, `/#` and `/%` too). Not
/// `${name#…}`, `${name:-…}`, `${ROOT}/x`, or GitHub's `${{ … }}`. A line
/// whose first non-blank character is `#` is a comment and never matches.
fn has_substitution(line: &str) -> bool {
    if line.trim_start().starts_with('#') {
        return false;
    }
    let chars: Vec<char> = line.chars().collect();
    for (i, w) in chars.windows(2).enumerate() {
        if w != ['$', '{'] {
            continue;
        }
        let mut j = i + 2;
        // The identifier starts right after `${`, so `${{` and `${#x}`
        // don't match.
        if !chars
            .get(j)
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == '_')
        {
            continue;
        }
        while chars.get(j).copied().is_some_and(is_ident) {
            j += 1;
        }
        if chars.get(j) == Some(&'[') {
            match chars[j..].iter().position(|c| *c == ']') {
                Some(close) => j += close + 1,
                None => continue,
            }
        }
        if chars.get(j) == Some(&'/') {
            return true;
        }
    }
    false
}

/// Every `*.sh` under `dir`, recursively.
fn shell_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            shell_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "sh") {
            out.push(p);
        }
    }
}

fn offenders() -> Vec<String> {
    let root = root();
    let mut files = Vec::new();
    shell_files(&root.join("scripts"), &mut files);
    let mut flows: Vec<_> = std::fs::read_dir(root.join(".github/workflows"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "yml"))
        .collect();
    flows.sort();
    files.extend(flows);
    assert!(!files.is_empty(), "found no shell to scan");
    let mut bad = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        let rel = f
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        for (n, line) in text.lines().enumerate() {
            if has_substitution(line) {
                bad.push(format!("{rel}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    bad
}

#[test]
fn ci_and_release_shell_has_no_bash_pattern_substitution() {
    let bad = offenders();
    assert!(
        bad.is_empty(),
        "bash pattern substitution (`${{var/…}}`, `${{var//…}}`) in shell that CI or a release runs:\n{}\n\n\
         Pipe the text through sed, tr or grep instead: under macOS bash 3.2, which the release \
         runner uses, ${{var//…}} on a large string took over an hour (scripts/changelog-notes.sh, \
         0.28.0). Dev and CI run bash 5, where it took about a second, so nothing there shows it. \
         For an emptiness test, use `grep -q '[^[:space:]]' <<<\"$var\"` (not `printf … | grep -q` under \
         `set -o pipefail`: grep exits at the first match, printf dies of SIGPIPE once the text \
         is larger than the pipe buffer, and the pipeline reports failure). There is no allow list.",
        bad.join("\n")
    );
}

#[test]
fn the_matcher_flags_substitution_and_nothing_else() {
    // The exact line from scripts/changelog-notes.sh at 1b8c647.
    assert!(has_substitution(
        r#"if [ -z "${notes//[[:space:]]/}" ]; then"#
    ));
    for flagged in [
        "x=${a/b/c}",
        "x=${a//b}",
        "x=${a/#b/c}",
        "x=${a/%b/c}",
        "x=${arr[0]//b/c}",
        "x=${arr[@]/b}",
        "  echo \"${_under//x/y}\"",
        "echo ${ok} ${a1/b}",
    ] {
        assert!(has_substitution(flagged), "should flag: {flagged}");
    }
    for fine in [
        "x=${a#pre}",
        "x=${a##pre}",
        "x=${a%suf}",
        "x=${a%%suf}",
        "x=${a:-d}",
        "x=${a:?msg}",
        "x=${a:1:2}",
        "x=${#a}",
        "x=${ROOT}/scripts",
        "x=$ROOT/x",
        "x=\"${1:?usage: x}\"",
        "run: echo ${{ github.ref }}/x",
        "if: ${{ matrix.os == 'macos' }}",
        "x=${arr[0]}/y",
        "    # grep, not ${notes//[[:space:]]/}: under macOS's bash 3.2 (the release",
        "#${a//b}",
        "",
    ] {
        assert!(!has_substitution(fine), "should not flag: {fine}");
    }
    // The comment now in the notes script is skipped as a comment.
    let script = std::fs::read_to_string(root().join("scripts/changelog-notes.sh")).unwrap();
    assert!(
        script.lines().any(|l| l.contains("${notes//")),
        "the script's comment no longer names the old form; update this test"
    );
    assert!(script.lines().all(|l| !has_substitution(l)));
}

#[cfg(unix)]
mod notes_script {
    use super::root;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    /// Healthy is well under a second; this only stops the polling loop.
    const LIMIT: Duration = Duration::from_secs(60);

    struct Run {
        ok: bool,
        stdout: String,
    }

    /// Runs `script` (the text of `changelog-notes.sh`) under `/bin/bash`
    /// (not via the shebang: on macOS that is bash 3.2) against `changelog`,
    /// for `version`. Panics if it hasn't exited after `limit`.
    fn run_script(script: &str, changelog: &str, version: &str, limit: Duration) -> Run {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("scripts")).unwrap();
        let path = dir.path().join("scripts/changelog-notes.sh");
        std::fs::write(&path, script).unwrap();
        std::fs::write(dir.path().join("CHANGELOG.md"), changelog).unwrap();
        // To a file, not a pipe: nothing reads a pipe while we poll.
        let out_path = dir.path().join("stdout.txt");
        let out = std::fs::File::create(&out_path).unwrap();
        let mut child = Command::new("/bin/bash")
            .arg(&path)
            .arg(version)
            .stdout(out)
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut status = None;
        let exited = blyg_core::wait::poll_until(limit, Duration::from_millis(20), || {
            status = child.try_wait().unwrap();
            status.is_some()
        });
        if !exited {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "scripts/changelog-notes.sh did not finish within {limit:?} under /bin/bash on a \
                 large changelog section: it is too slow. The release runner uses macOS bash 3.2, \
                 where bash pattern substitution (`${{var//…}}`) on a large string took over an \
                 hour (0.28.0). Pipe the text through sed, tr or grep instead."
            );
        }
        Run {
            ok: status.unwrap().success(),
            stdout: std::fs::read_to_string(out_path).unwrap(),
        }
    }

    fn repo_script() -> String {
        std::fs::read_to_string(root().join("scripts/changelog-notes.sh")).unwrap()
    }

    /// A changelog whose 2.0.0 section is about 200 KB, between two small
    /// ones; with the big section's first and last invented line.
    fn big_changelog() -> (String, String, String) {
        let mut body = Vec::new();
        let mut size = 0;
        let mut i = 0;
        while size < 200_000 {
            let line = format!("- invented entry {i:05}: alpha beta gamma delta epsilon");
            size += line.len() + 1;
            body.push(line);
            i += 1;
        }
        let first = body[0].clone();
        let last = body[body.len() - 1].clone();
        let log = format!(
            "# Changelog\n\n## [3.0.0]\n\n- NEIGHBOUR-NEWER line\n\n\
             ## [2.0.0]\n\n### Added\n\n{}\n\n\
             ## [1.0.0]\n\n- NEIGHBOUR-OLDER line\n",
            body.join("\n")
        );
        (log, first, last)
    }

    #[test]
    fn notes_script_finishes_on_a_large_section() {
        let (log, first, last) = big_changelog();
        let run = run_script(&repo_script(), &log, "2.0.0", LIMIT);
        assert!(
            run.ok,
            "scripts/changelog-notes.sh exited non-zero on a section of about 200 KB that is \
             there, so a release with large notes would stop. A likely cause: under \
             `set -o pipefail`, `printf … | grep -q` fails once the text is larger than the pipe \
             buffer (grep exits at the first match and printf dies of SIGPIPE). Feed grep with a \
             here-string (`grep -q … <<<\"$notes\"`) or drop `-q` and send its output to /dev/null."
        );
        assert!(run.stdout.contains(&first), "first line missing");
        assert!(run.stdout.contains(&last), "last line missing");
        assert!(
            !run.stdout.contains("NEIGHBOUR"),
            "a neighbouring section leaked"
        );

        let missing = run_script(&repo_script(), &log, "9.9.9", LIMIT);
        assert!(!missing.ok, "a version with no section must fail");
    }

    #[test]
    fn notes_script_refuses_a_section_of_blank_lines() {
        let log = "# Changelog\n\n## [2.0.0]\n\n\n   \n\n## [1.0.0]\n\n- older\n";
        let run = run_script(&repo_script(), log, "2.0.0", LIMIT);
        assert!(!run.ok, "an empty section must fail");
    }
}
