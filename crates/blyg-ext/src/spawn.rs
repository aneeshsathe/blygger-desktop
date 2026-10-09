//! Starting an extension's process: finding its program (`.exe`/`.cmd` on
//! Windows), the scrubbed environment it gets, and its stderr log.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The only environment variables an extension sees (when set). Nothing
/// else is passed: no `BLYGGER_*`, no tokens, no proxy credentials.
pub const KEEP_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TMPDIR",
    // Windows
    "USERPROFILE",
    "USERNAME",
    "APPDATA",
    "LOCALAPPDATA",
    "TEMP",
    "TMP",
    "SystemRoot",
    "SystemDrive",
    "windir",
    "ComSpec",
    "PATHEXT",
];

/// Directories added to the end of `PATH`: a GUI app's PATH is minimal, and
/// `python3`/`node` usually live here.
#[cfg(target_os = "macos")]
const EXTRA_PATH: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"];
#[cfg(all(unix, not(target_os = "macos")))]
const EXTRA_PATH: &[&str] = &["/usr/local/bin", "/usr/bin", "/bin"];
#[cfg(not(unix))]
const EXTRA_PATH: &[&str] = &[];

/// The child's environment, from `get` (the parent's `std::env::var_os`):
/// [`KEEP_ENV`] only, with [`EXTRA_PATH`] appended to `PATH`. Anything
/// named `BLYGGER_*` is dropped even if it were listed.
pub fn scrubbed_env(get: impl Fn(&str) -> Option<OsString>) -> Vec<(String, OsString)> {
    let mut out = vec![];
    for &k in KEEP_ENV {
        if k.to_ascii_uppercase().starts_with("BLYGGER_") {
            continue;
        }
        let Some(v) = get(k) else { continue };
        if k == "PATH" {
            let mut dirs: Vec<PathBuf> = std::env::split_paths(&v).collect();
            for d in EXTRA_PATH {
                let d = PathBuf::from(d);
                if !dirs.contains(&d) {
                    dirs.push(d);
                }
            }
            out.push((k.to_string(), std::env::join_paths(dirs).unwrap_or(v)));
        } else {
            out.push((k.to_string(), v));
        }
    }
    if !out.iter().any(|(k, _)| k == "PATH") && !EXTRA_PATH.is_empty() {
        let dirs = EXTRA_PATH.iter().map(PathBuf::from);
        if let Ok(p) = std::env::join_paths(dirs) {
            out.push(("PATH".into(), p));
        }
    }
    out
}

/// The `PATH` directories in `env` (from [`scrubbed_env`]).
pub fn path_dirs(env: &[(String, OsString)]) -> Vec<PathBuf> {
    env.iter()
        .find(|(k, _)| k == "PATH")
        .map(|(_, v)| std::env::split_paths(v).collect())
        .unwrap_or_default()
}

/// Windows executable extensions, from `PATHEXT` (default `.COM;.EXE;.BAT;.CMD`),
/// lower-case with the dot.
pub fn exe_extensions(pathext: Option<&str>) -> Vec<String> {
    let p = pathext
        .filter(|p| !p.trim().is_empty())
        .unwrap_or(".COM;.EXE;.BAT;.CMD");
    p.split(';')
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| e.starts_with('.') && e.len() > 1)
        .collect()
}

/// Find the program `argv0` names.
///
/// - A path (contains `/`, or `\` on Windows, or starts with `.`) is taken
///   relative to `ext_dir` (the extension's folder) unless absolute.
/// - A bare name is searched in `dirs` (the child's `PATH`).
/// - On Windows (`windows`), a name without an extension also tries each of
///   `exts` (`.exe`, `.cmd`, …), as `cmd.exe` would.
///
/// `is_file` says whether a candidate exists and is runnable (injected so
/// the Windows rules are testable anywhere).
pub fn resolve_program(
    argv0: &str,
    ext_dir: Option<&Path>,
    dirs: &[PathBuf],
    windows: bool,
    exts: &[String],
    is_file: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let argv0 = argv0.trim();
    if argv0.is_empty() {
        return None;
    }
    let has_sep = argv0.contains('/') || (windows && argv0.contains('\\'));
    let has_ext = {
        let last = argv0.rsplit(['/', '\\']).next().unwrap_or(argv0);
        last.contains('.') && !last.starts_with('.')
    };
    let variants = |base: PathBuf| -> Vec<PathBuf> {
        let mut v = vec![];
        if windows && !has_ext {
            for e in exts {
                let mut s = base.clone().into_os_string();
                s.push(e);
                v.push(PathBuf::from(s));
            }
        }
        v.push(base);
        v
    };
    if has_sep || argv0.starts_with('.') {
        let rel: PathBuf = if windows {
            argv0.split(['/', '\\']).collect()
        } else {
            PathBuf::from(argv0)
        };
        let base = if is_absolute(argv0, windows) {
            PathBuf::from(argv0)
        } else {
            match ext_dir {
                Some(d) => d.join(rel),
                None => rel,
            }
        };
        return variants(base).into_iter().find(|p| is_file(p));
    }
    dirs.iter()
        .flat_map(|d| variants(d.join(argv0)))
        .find(|p| is_file(p))
}

fn is_absolute(p: &str, windows: bool) -> bool {
    if windows {
        let b = p.as_bytes();
        p.starts_with("\\\\") || (b.len() >= 3 && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
    } else {
        p.starts_with('/')
    }
}

/// A runnable file on this platform.
pub fn is_runnable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// The largest stderr log before it's rotated to `stderr.log.1`.
pub const STDERR_LOG_MAX: u64 = 1024 * 1024;
/// Lines of stderr kept in memory for a "didn't start" notice.
pub const STDERR_TAIL: usize = 20;

/// An extension's stderr: appended to `<storage>/stderr.log` (rotated at
/// [`STDERR_LOG_MAX`]), the last [`STDERR_TAIL`] lines kept in memory.
#[derive(Clone)]
pub struct StderrLog {
    inner: Arc<Mutex<LogInner>>,
}

struct LogInner {
    path: PathBuf,
    file: Option<File>,
    size: u64,
    max: u64,
    tail: VecDeque<String>,
}

impl StderrLog {
    pub fn new(path: PathBuf, max: u64) -> StderrLog {
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok();
        StderrLog {
            inner: Arc::new(Mutex::new(LogInner {
                path,
                file,
                size,
                max,
                tail: VecDeque::new(),
            })),
        }
    }

    pub fn push(&self, line: &str) {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let line = line.trim_end_matches(['\r', '\n']);
        if g.tail.len() == STDERR_TAIL {
            g.tail.pop_front();
        }
        g.tail.push_back(line.to_string());
        let bytes = line.len() as u64 + 1;
        if g.size + bytes > g.max {
            g.file = None;
            let old = g.path.with_extension("log.1");
            let _ = std::fs::rename(&g.path, old);
            g.file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&g.path)
                .ok();
            g.size = 0;
        }
        if let Some(f) = g.file.as_mut()
            && writeln!(f, "{line}").is_ok()
        {
            g.size += bytes;
        }
    }

    /// The last lines, oldest first.
    pub fn tail(&self) -> Vec<String> {
        let g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        g.tail.iter().cloned().collect()
    }

    /// Start a thread copying `r` (the child's stderr) into the log.
    pub fn drain(&self, r: impl std::io::Read + Send + 'static) {
        use std::io::BufRead;
        let me = self.clone();
        let _ = std::thread::Builder::new()
            .name("bxp-stderr".into())
            .spawn(move || {
                let mut r = std::io::BufReader::new(r);
                let mut buf = Vec::new();
                loop {
                    buf.clear();
                    match r.read_until(b'\n', &mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => me.push(&String::from_utf8_lossy(&buf)),
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn env_keeps_only_the_list() {
        let parent: HashMap<&str, &str> = [
            ("PATH", "/usr/bin"),
            ("HOME", "/home/someone"),
            ("BLYGGER_FAKE", "1"),
            ("BLYGGER_DATA_DIR", "/secret"),
            ("AWS_SECRET_ACCESS_KEY", "nope"),
            ("SystemRoot", "C:\\Windows"),
            ("TEMP", "C:\\Temp"),
            ("USERPROFILE", "C:\\Users\\someone"),
        ]
        .into();
        let env = scrubbed_env(|k| parent.get(k).map(OsString::from));
        let keys: HashSet<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains("HOME") && keys.contains("SystemRoot") && keys.contains("TEMP"));
        assert!(keys.contains("USERPROFILE"));
        assert!(!keys.iter().any(|k| k.starts_with("BLYGGER_")));
        assert!(!keys.contains("AWS_SECRET_ACCESS_KEY"));
        assert!(path_dirs(&env).contains(&PathBuf::from("/usr/bin")));
    }

    #[test]
    fn pathext_defaults_and_parses() {
        assert_eq!(exe_extensions(None), [".com", ".exe", ".bat", ".cmd"]);
        assert_eq!(exe_extensions(Some(".EXE;.CMD;;junk")), [".exe", ".cmd"]);
    }

    #[test]
    fn resolves_bare_names_on_path_unix() {
        let files: HashSet<PathBuf> = [PathBuf::from("/b/python3")].into();
        let dirs = [PathBuf::from("/a"), PathBuf::from("/b")];
        let found = resolve_program("python3", None, &dirs, false, &[], |p| files.contains(p));
        assert_eq!(found, Some(PathBuf::from("/b/python3")));
        assert_eq!(
            resolve_program("ruby", None, &dirs, false, &[], |p| files.contains(p)),
            None
        );
    }

    #[test]
    fn resolves_relative_paths_against_the_extension_folder() {
        let dir = PathBuf::from("/ext/hello");
        let files: HashSet<PathBuf> = [dir.join("bin").join("run")].into();
        let found = resolve_program("./bin/run", Some(&dir), &[], false, &[], |p| {
            files.contains(&PathBuf::from(p.to_string_lossy().replace("/./", "/")))
        });
        assert!(found.is_some());
    }

    #[test]
    fn windows_tries_pathext_and_backslashes() {
        let exts = exe_extensions(None);
        let dirs = [PathBuf::from("bin-a"), PathBuf::from("bin-b")];
        // `node` is a .cmd shim in the second dir; `.exe` is preferred where both exist.
        let files: HashSet<PathBuf> = [
            PathBuf::from("bin-b").join("node.cmd"),
            PathBuf::from("bin-a").join("tool.cmd"),
            PathBuf::from("bin-a").join("tool.exe"),
        ]
        .into();
        let has = |p: &Path| files.contains(p);
        assert_eq!(
            resolve_program("node", None, &dirs, true, &exts, has),
            Some(PathBuf::from("bin-b").join("node.cmd"))
        );
        assert_eq!(
            resolve_program("tool", None, &dirs, true, &exts, has),
            Some(PathBuf::from("bin-a").join("tool.exe"))
        );
        // An explicit extension is used as is.
        assert_eq!(
            resolve_program("tool.cmd", None, &dirs, true, &exts, has),
            Some(PathBuf::from("bin-a").join("tool.cmd"))
        );
        // `bin\run` inside the extension folder, written with a backslash.
        let ext = PathBuf::from("ext");
        let files2: HashSet<PathBuf> = [ext.join("bin").join("run.exe")].into();
        assert_eq!(
            resolve_program("bin\\run", Some(&ext), &[], true, &exts, |p| files2
                .contains(p)),
            Some(ext.join("bin").join("run.exe"))
        );
        assert!(is_absolute("C:\\Tools\\x.exe", true));
        assert!(is_absolute("\\\\server\\share\\x.exe", true));
        assert!(!is_absolute("bin\\x.exe", true));
    }

    #[test]
    fn stderr_log_rotates_and_keeps_a_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stderr.log");
        let log = StderrLog::new(path.clone(), 100);
        for i in 0..30 {
            log.push(&format!("line {i:02} ......\r\n"));
        }
        let tail = log.tail();
        assert_eq!(tail.len(), STDERR_TAIL);
        assert_eq!(tail.last().unwrap(), "line 29 ......");
        assert!(std::fs::metadata(&path).unwrap().len() <= 100);
        assert!(path.with_extension("log.1").exists());
    }
}
