//! A folder of Markdown notes (an Obsidian vault), on its own: list, search,
//! read, create and edit. Nothing here knows about blygs, items or the
//! extension protocol, so it works with no blyg connected (or outside
//! Burrow).
//!
//! A note's id is its path relative to the vault with `/` separators
//! (`Inbox/Tide tables.md`); a `\` in an id is read as a separator too, so
//! Windows-style paths work. Ids that climb out of the vault (`..`, an
//! absolute path) are refused.
//!
//! Safety rules: an edit names the hash of the text it started from and is
//! refused if the file changed on disk since (never clobber); a new note
//! never overwrites an existing file (`Name 2.md`); nothing is ever deleted.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use blyg_core::content_hash;

use crate::frontmatter;

/// Files read as notes.
pub const NOTE_EXTENSIONS: &[&str] = &["md", "markdown"];
/// The most notes indexed (a cap so a huge folder can't stall the poll).
pub const MAX_NOTES: usize = 20_000;
/// The deepest folder walked.
pub const MAX_DEPTH: usize = 24;
/// Characters of a search hit's excerpt (as the quote picker's).
pub const EXCERPT_CHARS: usize = blyg_core::pick::EXCERPT_CHARS;
/// The longest file name made for a new note (without `.md`).
pub const NAME_MAX: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    /// No such note (or folder).
    NotFound,
    /// The note changed on disk since it was read; `current` is its hash now.
    Stale {
        current: String,
    },
    /// A bad id, folder or request.
    Invalid(String),
    Io(String),
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VaultError::NotFound => f.write_str("no such note"),
            VaultError::Stale { .. } => f.write_str("the note changed on disk since it was read"),
            VaultError::Invalid(m) | VaultError::Io(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for VaultError {}

fn io(e: std::io::Error) -> VaultError {
    if e.kind() == std::io::ErrorKind::NotFound {
        VaultError::NotFound
    } else {
        VaultError::Io(e.to_string())
    }
}

/// A folder or note, one level of a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub is_dir: bool,
    /// RFC 3339 UTC.
    pub modified: Option<String>,
}

/// A note as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub id: String,
    pub title: String,
    /// The whole file.
    pub markdown: String,
    /// Without frontmatter.
    pub body: String,
    /// `sha256:<hex>` of the file's bytes.
    pub hash: String,
}

/// A search hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub id: String,
    pub title: String,
    pub excerpt: String,
    pub modified: Option<String>,
}

#[derive(Debug, Clone)]
struct Indexed {
    title: String,
    excerpt: String,
    /// Lower-cased title + body, for matching.
    haystack: String,
    title_lc: String,
    mtime: Option<SystemTime>,
    size: u64,
}

/// An open vault and its in-memory index.
pub struct Vault {
    root: PathBuf,
    index: HashMap<String, Indexed>,
}

impl Vault {
    /// Open the folder at `root` (it must exist) and index it.
    pub fn open(root: impl Into<PathBuf>) -> Result<Vault, VaultError> {
        let root = root.into();
        if !root.is_dir() {
            return Err(VaultError::Invalid(format!(
                "{} isn't a folder",
                root.display()
            )));
        }
        let mut v = Vault {
            root,
            index: HashMap::new(),
        };
        v.refresh();
        Ok(v)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Re-walk the folder; files whose size and mtime are unchanged aren't
    /// re-read. True when anything changed.
    pub fn refresh(&mut self) -> bool {
        let mut found: Vec<(String, PathBuf, Option<SystemTime>, u64)> = vec![];
        walk(&self.root, "", 0, &mut found);
        let mut changed = found.len() != self.index.len();
        let mut next = HashMap::with_capacity(found.len());
        for (id, path, mtime, size) in found {
            if let Some(old) = self.index.get(&id)
                && old.mtime == mtime
                && old.size == size
            {
                next.insert(id, old.clone());
                continue;
            }
            changed = true;
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue; // not UTF-8 text, or gone
            };
            next.insert(id.clone(), index_note(&id, &text, mtime, size));
        }
        self.index = next;
        changed
    }

    /// One folder's notes and subfolders (`folder` `None` = the top):
    /// folders first, then notes, each by name. Hidden entries are skipped.
    pub fn list(&self, folder: Option<&str>) -> Result<Vec<Entry>, VaultError> {
        let (rel, dir) = match folder.map(str::trim).filter(|f| !f.is_empty() && *f != "/") {
            Some(f) => {
                let rel = normalise(f)?;
                let dir = self.path_of(&rel);
                (rel, dir)
            }
            None => (String::new(), self.root.clone()),
        };
        let rd = std::fs::read_dir(&dir).map_err(io)?;
        let mut dirs = vec![];
        let mut notes = vec![];
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let id = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            let meta = e.metadata().ok();
            let modified = meta.as_ref().and_then(|m| m.modified().ok()).map(rfc3339);
            if e.path().is_dir() {
                dirs.push(Entry {
                    id,
                    title: name,
                    is_dir: true,
                    modified,
                });
            } else if is_note(&name) {
                let title = self
                    .index
                    .get(&id)
                    .map(|i| i.title.clone())
                    .unwrap_or_else(|| stem(&name).to_string());
                notes.push(Entry {
                    id,
                    title,
                    is_dir: false,
                    modified,
                });
            }
        }
        dirs.sort_by_key(|a| a.id.to_lowercase());
        notes.sort_by_key(|a| a.id.to_lowercase());
        dirs.extend(notes);
        Ok(dirs)
    }

    /// Notes matching every word of `query` in their title or text: title
    /// matches first, then the most recently modified. An empty query lists
    /// the most recent.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let mut hits: Vec<(&String, &Indexed, bool)> = self
            .index
            .iter()
            .filter(|(_, n)| words.iter().all(|w| n.haystack.contains(w.as_str())))
            .map(|(id, n)| {
                (
                    id,
                    n,
                    !words.is_empty() && words.iter().all(|w| n.title_lc.contains(w.as_str())),
                )
            })
            .collect();
        hits.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then(b.1.mtime.cmp(&a.1.mtime))
                .then(a.0.cmp(b.0))
        });
        hits.into_iter()
            .take(limit)
            .map(|(id, n, _)| Hit {
                id: id.clone(),
                title: n.title.clone(),
                excerpt: n.excerpt.clone(),
                modified: n.mtime.map(rfc3339),
            })
            .collect()
    }

    /// Read a note.
    pub fn read(&self, id: &str) -> Result<Note, VaultError> {
        let id = normalise(id)?;
        if !is_note(&id) {
            return Err(VaultError::Invalid(format!("{id} isn't a Markdown note")));
        }
        let text = std::fs::read_to_string(self.path_of(&id)).map_err(|e| match e.kind() {
            std::io::ErrorKind::InvalidData => {
                VaultError::Invalid(format!("{id} isn't UTF-8 text"))
            }
            _ => io(e),
        })?;
        Ok(Note {
            title: title_of(&id, &text),
            body: frontmatter::body(&text).to_string(),
            hash: content_hash(&text),
            markdown: text,
            id,
        })
    }

    /// Overwrite note `id` with `markdown`, only if its file still hashes to
    /// `base_hash`. Written to a temporary file and renamed into place.
    pub fn save(&mut self, id: &str, markdown: &str, base_hash: &str) -> Result<Note, VaultError> {
        let current = self.read(id)?;
        if current.hash != base_hash {
            return Err(VaultError::Stale {
                current: current.hash,
            });
        }
        let path = self.path_of(&current.id);
        let tmp = path.with_file_name(format!(
            ".{}.burrow-tmp",
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
        std::fs::write(&tmp, markdown).map_err(io)?;
        if let Err(e) = std::fs::rename(&tmp, &path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(io(e));
        }
        self.refresh();
        self.read(&current.id)
    }

    /// Create a new note in `folder` (`None` = the top; created if missing),
    /// named from `title` or else the text's first line. Never overwrites:
    /// a taken name becomes `Name 2.md`, `Name 3.md`, …
    pub fn create(
        &mut self,
        folder: Option<&str>,
        title: Option<&str>,
        markdown: &str,
    ) -> Result<Note, VaultError> {
        let rel = match folder.map(str::trim).filter(|f| !f.is_empty() && *f != "/") {
            Some(f) => normalise(f)?,
            None => String::new(),
        };
        let dir = self.path_of(&rel);
        std::fs::create_dir_all(&dir).map_err(io)?;
        let base = file_name_for(title.unwrap_or(""), markdown);
        for n in 1..1000 {
            let name = if n == 1 {
                format!("{base}.md")
            } else {
                format!("{base} {n}.md")
            };
            let path = dir.join(&name);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    f.write_all(markdown.as_bytes()).map_err(io)?;
                    drop(f);
                    let id = if rel.is_empty() {
                        name
                    } else {
                        format!("{rel}/{name}")
                    };
                    self.refresh();
                    return self.read(&id);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(io(e)),
            }
        }
        Err(VaultError::Io(format!("too many notes named {base}")))
    }

    fn path_of(&self, rel: &str) -> PathBuf {
        let mut p = self.root.clone();
        for part in rel.split('/').filter(|s| !s.is_empty()) {
            p.push(part);
        }
        p
    }
}

/// An id or folder as `a/b/c.md`: `\` read as `/`, `.` parts dropped;
/// `..`, absolute paths and drive letters refused.
pub fn normalise(id: &str) -> Result<String, VaultError> {
    let s = id.trim().replace('\\', "/");
    if s.starts_with('/') || s.as_bytes().get(1) == Some(&b':') {
        return Err(VaultError::Invalid(format!(
            "{id} is outside the notes folder"
        )));
    }
    let mut parts = vec![];
    for part in s.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                return Err(VaultError::Invalid(format!(
                    "{id} is outside the notes folder"
                )));
            }
            p => parts.push(p),
        }
    }
    if parts.is_empty() {
        return Err(VaultError::Invalid("empty path".into()));
    }
    // Belt and braces: the joined path has only normal components.
    if Path::new(&parts.join("/"))
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(VaultError::Invalid(format!(
            "{id} is outside the notes folder"
        )));
    }
    Ok(parts.join("/"))
}

/// A file name for a new note: `title`, else the first line of text with
/// Markdown heading marks removed; characters file systems refuse (on any
/// platform) dropped; at most [`NAME_MAX`]; "Untitled" when nothing's left.
pub fn file_name_for(title: &str, markdown: &str) -> String {
    let src = if title.trim().is_empty() {
        let body = frontmatter::body(markdown);
        body.lines()
            .map(|l| {
                l.trim()
                    .trim_start_matches('#')
                    .trim()
                    .trim_start_matches('>')
                    .trim()
            })
            .find(|l| !l.is_empty())
            .unwrap_or("")
            .to_string()
    } else {
        title.trim().to_string()
    };
    let cleaned: String = src
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .filter(|c| {
            !matches!(
                c,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']'
            )
        })
        .collect();
    let mut name: String = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.chars().count() > NAME_MAX {
        name = name
            .chars()
            .take(NAME_MAX)
            .collect::<String>()
            .trim_end()
            .to_string();
    }
    let name = name.trim_matches(['.', ' ']).to_string();
    // Windows reserves these names in every folder.
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "LPT1", "LPT2", "LPT3",
    ];
    if name.is_empty() || reserved.contains(&name.to_ascii_uppercase().as_str()) {
        "Untitled".into()
    } else {
        name
    }
}

fn is_note(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| NOTE_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

fn stem(name: &str) -> &str {
    let file = name.rsplit('/').next().unwrap_or(name);
    file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file)
}

/// A note's title: frontmatter `title:`, else the file name (as Obsidian).
pub fn title_of(id: &str, text: &str) -> String {
    frontmatter::scalar(text, "title").unwrap_or_else(|| stem(id).to_string())
}

fn index_note(id: &str, text: &str, mtime: Option<SystemTime>, size: u64) -> Indexed {
    let title = title_of(id, text);
    let body = frontmatter::body(text);
    let plain = blyg_core::plain_text(body);
    let excerpt = if plain.chars().count() <= EXCERPT_CHARS {
        plain.clone()
    } else {
        let cut: String = plain.chars().take(EXCERPT_CHARS - 1).collect();
        format!("{}…", cut.trim_end())
    };
    Indexed {
        haystack: format!("{}\n{}", title, body).to_lowercase(),
        title_lc: title.to_lowercase(),
        title,
        excerpt,
        mtime,
        size,
    }
}

fn walk(
    dir: &Path,
    rel: &str,
    depth: usize,
    out: &mut Vec<(String, PathBuf, Option<SystemTime>, u64)>,
) {
    if depth > MAX_DEPTH || out.len() >= MAX_NOTES {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if out.len() >= MAX_NOTES {
            return;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue; // .obsidian, .git, .trash, our temp files
        }
        let id = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            walk(&e.path(), &id, depth + 1, out);
        } else if ft.is_file() && is_note(&name) {
            let meta = e.metadata().ok();
            out.push((
                id,
                e.path(),
                meta.as_ref().and_then(|m| m.modified().ok()),
                meta.map(|m| m.len()).unwrap_or(0),
            ));
        }
    }
}

/// `SystemTime` as RFC 3339 UTC, to the second.
pub fn rfc3339(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn ids_normalise_and_never_escape() {
        assert_eq!(
            normalise("Inbox\\Tide tables.md").unwrap(),
            "Inbox/Tide tables.md"
        );
        assert_eq!(normalise("./a//b.md").unwrap(), "a/b.md");
        for bad in [
            "../x.md",
            "a/../../x.md",
            "/etc/passwd",
            "C:\\x.md",
            "\\\\server\\x.md",
            "",
        ] {
            assert!(normalise(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn file_names_are_safe_everywhere() {
        assert_eq!(file_name_for("", "# Tide: tables?\nbody"), "Tide tables");
        assert_eq!(file_name_for("A/B\\C", ""), "ABC");
        assert_eq!(
            file_name_for("", "---\ntitle: x\n---\n\n> quoted start"),
            "quoted start"
        );
        assert_eq!(file_name_for("", ""), "Untitled");
        assert_eq!(file_name_for("con", ""), "Untitled");
        assert_eq!(file_name_for("...", ""), "Untitled");
        assert_eq!(file_name_for(&"x".repeat(200), "").len(), NAME_MAX);
    }

    #[test]
    fn rfc3339_known_dates() {
        assert_eq!(rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(
            rfc3339(UNIX_EPOCH + Duration::from_secs(1_709_210_096)),
            "2024-02-29T12:34:56Z"
        );
    }
}
