//! --- studio 0.71 --- What a generation read, UI-free: for each generated
//! `[TK]` scope, the posts it drew on and the web pages its model read
//! (studio 0.70, decision #117), and which of them tell another blyg when
//! the post is published (`ScopeProvenance::notifies`: a post on another
//! blyg; a web page or your own post never tells anyone). The writer shows
//! it in the publish sheet and under the preview, the reader under a post
//! whose generated text the publisher disclosed.
//!
//! A post is named by its title when it's held here (linked to it), else
//! by its blyg ("a post on kit.blyg.example.com"): never by its id.

use blyg_core::{Backend, Item, LocalId, PageSource, ProvenanceSource, ScopeProvenance};

#[cfg(test)]
#[path = "read_tests.rs"]
mod read_tests;

/// The mark on an entry that tells its blyg.
pub const TELLS: &str = "tells its author when you publish";
/// The one line under the heading: every entry without a mark tells no one.
pub const TELLS_NO_ONE: &str = "Web pages and your own posts tell no one.";
/// The heading over the list.
pub const HEADING: &str = "What generation read";

/// Where a post's title goes when clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Open {
    /// One of your own posts: open it here.
    Own(LocalId),
    /// Another blyg's post: its page.
    Web(String),
}

/// A post as held here: its title, and where it opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    pub title: String,
    pub open: Option<Open>,
}

/// One thing a scope read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// "drew on “Title” (blyg.example.com)", "read Example News", "drew on
    /// a post on blyg.example.com". Never a post's id.
    pub what: String,
    /// The part of `what` that links to the post (its quoted title, or
    /// "a post"), and where it goes.
    pub link: Option<(String, Open)>,
    /// A web page's address.
    pub url: Option<String>,
    /// It tells another blyg (a mention) when the post is published.
    pub tells: bool,
}

fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url).ok()?.host_str().map(str::to_string)
}

/// One web page: its site, when that says more than its host, then its
/// address.
pub fn page(p: &PageSource) -> Entry {
    let site = p.site();
    let bare = |h: &str| h.trim_start_matches("www.").to_ascii_lowercase();
    let same = site == p.url || host_of(&p.url).is_some_and(|h| bare(&h) == bare(&site));
    Entry {
        what: if same {
            "read".into()
        } else {
            format!("read {site}")
        },
        link: None,
        url: Some(p.url.clone()),
        tells: false,
    }
}

/// One post. `named`: its title as held here, else `None`. `yours`: the
/// list is under your own post, so a post of the same blyg is yours.
pub fn post(s: &ProvenanceSource, named: Option<Named>, yours: bool) -> Entry {
    let cited = s.cited.as_ref();
    let site = cited.and_then(|c| c.get("source")?.as_str().map(str::to_string));
    let from = site.or_else(|| s.origin.as_deref().and_then(host_of));
    // The address the blyg cited for it, if any.
    let cited_url = cited
        .and_then(|c| c.get("url")?.as_str().map(str::to_string))
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"));
    let tells = s.origin.is_some();
    let Some(n) = named else {
        let what = match (&s.origin, &from) {
            (Some(_), Some(f)) => format!("drew on a post on {f}"),
            (Some(_), None) => "drew on a post of another blyg".to_string(),
            (None, _) => "drew on a post of this blyg".to_string(),
        };
        return Entry {
            what,
            link: cited_url.map(|u| ("a post".to_string(), Open::Web(u))),
            url: None,
            tells,
        };
    };
    let quoted = format!("“{}”", n.title);
    let what = match (&s.origin, &from) {
        (Some(_), Some(f)) => format!("drew on {quoted} ({f})"),
        (None, _) if yours => format!("drew on your post {quoted}"),
        _ => format!("drew on {quoted}"),
    };
    Entry {
        what,
        link: n
            .open
            .or_else(|| cited_url.map(Open::Web))
            .map(|o| (quoted, o)),
        url: None,
        tells,
    }
}

/// Names the posts a list mentions from what this backend holds. `home`:
/// the blyg whose post the list is under (`None`: your own), where a
/// source without an origin is from. Your items are read once, and only
/// when one is asked for.
pub struct Names<'a> {
    backend: &'a dyn Backend,
    home: Option<&'a str>,
    items: std::cell::OnceCell<Vec<Item>>,
}

impl<'a> Names<'a> {
    pub fn new(backend: &'a dyn Backend, home: Option<&'a str>) -> Self {
        Names {
            backend,
            home,
            items: std::cell::OnceCell::new(),
        }
    }

    fn own(&self, id: &str) -> Option<Named> {
        self.items
            .get_or_init(|| self.backend.items())
            .iter()
            .find(|i| i.server_id.as_ref().is_some_and(|sid| sid.0 == id))
            .map(|i| Named {
                title: crate::vm::item_title(i),
                open: Some(Open::Own(i.local_id.clone())),
            })
    }

    /// `s`'s title and where it opens, when it's held here.
    pub fn get(&self, s: &ProvenanceSource) -> Option<Named> {
        let Some(o) = s.origin.as_deref().or(self.home) else {
            return self.own(&s.id);
        };
        self.backend
            .reading_with_id(&blyg_core::RemoteId::from(s.id.as_str()))
            .into_iter()
            .find(|r| blyg_core::profile::same_origin(&r.origin, o))
            .map(|r| Named {
                title: crate::app::reading::vm::post_title(&r),
                open: crate::app::reading::vm::web_url(&r).map(Open::Web),
            })
            .or_else(|| {
                // Another blyg's list naming a post of yours.
                let mine = self.backend.base_url()?;
                if blyg_core::profile::same_origin(&mine, o) {
                    self.own(&s.id)
                } else {
                    None
                }
            })
    }
}

/// What one scope read: its posts in order, then its pages.
pub fn entries(
    p: &ScopeProvenance,
    name_of: &dyn Fn(&ProvenanceSource) -> Option<Named>,
    yours: bool,
) -> Vec<Entry> {
    let tells: Vec<&str> = p.notifies().map(|s| s.id.as_str()).collect();
    p.sources
        .iter()
        .map(|s| {
            let mut e = post(s, name_of(s), yours);
            e.tells = tells.contains(&s.id.as_str());
            e
        })
        .chain(p.pages.iter().map(page))
        .collect()
}

/// Every generated scope that read something: (its index among all the
/// post's gaps, what it read).
pub fn scopes(
    all: &[Option<ScopeProvenance>],
    name_of: &dyn Fn(&ProvenanceSource) -> Option<Named>,
    yours: bool,
) -> Vec<(usize, Vec<Entry>)> {
    all.iter()
        .enumerate()
        .filter_map(|(i, p)| Some((i, entries(p.as_ref()?, name_of, yours))))
        .filter(|(_, e)| !e.is_empty())
        .collect()
}

/// The most entries a list of what generation read shows: the blyg that
/// generated decides how many pages a scope read.
pub const SHOWN: usize = 50;

/// The first [`SHOWN`] entries of `scopes`, in order, and how many more
/// there are.
pub fn capped(scopes: Vec<(usize, Vec<Entry>)>) -> (Vec<(usize, Vec<Entry>)>, usize) {
    let (mut left, mut more) = (SHOWN, 0);
    let mut out = Vec::new();
    for (i, mut entries) in scopes {
        if entries.len() > left {
            more += entries.len() - left;
            entries.truncate(left);
        }
        left -= entries.len();
        if !entries.is_empty() {
            out.push((i, entries));
        }
    }
    (out, more)
}

/// The line after a capped list ("+10 more").
pub fn more(n: usize) -> String {
    format!("+{n} more")
}

/// A scope's heading in the list ("TK 2").
pub fn scope_label(index: usize) -> String {
    format!("TK {}", index + 1)
}

/// An entry's mark: only an entry that tells someone has one
/// ([`TELLS_NO_ONE`] says the rest once, under the heading).
pub fn mark(e: &Entry) -> Option<&'static str> {
    e.tells.then_some(TELLS)
}
