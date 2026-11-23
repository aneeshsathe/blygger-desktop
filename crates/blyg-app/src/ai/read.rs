//! --- studio 0.71 --- What a generation read, UI-free: for each generated
//! `[TK]` scope, the posts it drew on and the web pages its model read
//! (studio 0.70, decision #117), and which of them tell another blyg when
//! the post is published (`ScopeProvenance::notifies`: a post on another
//! blyg; a web page or your own post never tells anyone). The writer shows
//! it in the publish sheet and under the preview, the reader under a post
//! whose generated text the publisher disclosed.

use blyg_core::{PageSource, ProvenanceSource, ScopeProvenance};

/// The mark on an entry that tells its blyg.
pub const TELLS: &str = "tells its author when you publish";
/// The mark on a web page.
pub const TELLS_NO_ONE: &str = "tells no one";
/// The heading over the list.
pub const HEADING: &str = "What generation read";

/// One thing a scope read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// "drew on “Title” (blyg.example.com)", "read Example News".
    pub what: String,
    /// A web page's address.
    pub url: Option<String>,
    /// It tells another blyg (a mention) when the post is published.
    pub tells: bool,
}

/// One web page: its site, then its address.
pub fn page(p: &PageSource) -> Entry {
    Entry {
        what: format!("read {}", p.site()),
        url: Some(p.url.clone()),
        tells: false,
    }
}

/// One post: `name` is its title as held here, else `None`.
pub fn post(s: &ProvenanceSource, name: Option<String>) -> Entry {
    let host = s
        .origin
        .as_deref()
        .and_then(|o| url::Url::parse(o).ok()?.host_str().map(str::to_string));
    let cited = s
        .cited
        .as_ref()
        .and_then(|c| c.get("source")?.as_str().map(str::to_string));
    let name = name.unwrap_or_else(|| format!("[[{}]]", s.id));
    let what = match (host, cited) {
        (_, Some(site)) => format!("drew on “{name}” ({site})"),
        (Some(h), None) => format!("drew on “{name}” ({h})"),
        (None, None) => format!("drew on your post “{name}”"),
    };
    Entry {
        what,
        url: None,
        tells: s.origin.is_some(),
    }
}

/// What one scope read: its posts in order, then its pages.
pub fn entries(
    p: &ScopeProvenance,
    name_of: &dyn Fn(&ProvenanceSource) -> Option<String>,
) -> Vec<Entry> {
    let tells: Vec<&str> = p.notifies().map(|s| s.id.as_str()).collect();
    p.sources
        .iter()
        .map(|s| {
            let mut e = post(s, name_of(s));
            e.tells = tells.contains(&s.id.as_str());
            e
        })
        .chain(p.pages.iter().map(page))
        .collect()
}

/// Every generated scope that read something: (its index, what it read).
pub fn scopes(
    all: &[Option<ScopeProvenance>],
    name_of: &dyn Fn(&ProvenanceSource) -> Option<String>,
) -> Vec<(usize, Vec<Entry>)> {
    all.iter()
        .enumerate()
        .filter_map(|(i, p)| Some((i, entries(p.as_ref()?, name_of))))
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

/// An entry's mark: what publishing does with it.
pub fn mark(e: &Entry) -> &'static str {
    if e.tells { TELLS } else { TELLS_NO_ONE }
}

#[cfg(test)]
mod tests {
    use blyg_core::{PageSource, ProvenanceSource, ScopeProvenance};
    use serde_json::json;

    use super::*;

    fn prov() -> ScopeProvenance {
        ScopeProvenance {
            model: "m".into(),
            sources: vec![
                ProvenanceSource {
                    id: "01own".into(),
                    version: Some(1),
                    origin: None,
                    cited: None,
                    ..Default::default()
                },
                ProvenanceSource {
                    id: "01far".into(),
                    version: Some(2),
                    origin: Some("https://ada.example.com".into()),
                    cited: None,
                    ..Default::default()
                },
            ],
            pages: vec![PageSource {
                url: "https://news.example.com/a".into(),
                cited: json!({"source": "Example News", "url": "https://news.example.com/a"}),
                ..Default::default()
            }],
            at: None,
            ..Default::default()
        }
    }

    #[test]
    fn posts_then_pages_and_only_another_blygs_post_tells() {
        let name = |s: &ProvenanceSource| (s.id == "01own").then(|| "Fog notes".to_string());
        let e = entries(&prov(), &name);
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].what, "drew on your post “Fog notes”");
        assert!(!e[0].tells);
        assert_eq!(e[1].what, "drew on “[[01far]]” (ada.example.com)");
        assert!(e[1].tells);
        assert_eq!(mark(&e[1]), TELLS);
        assert_eq!(e[2].what, "read Example News");
        assert_eq!(e[2].url.as_deref(), Some("https://news.example.com/a"));
        assert!(!e[2].tells, "a web page never tells");
        assert_eq!(mark(&e[2]), TELLS_NO_ONE);
    }

    #[test]
    fn only_scopes_that_read_something_are_listed() {
        let bare = ScopeProvenance {
            sources: vec![],
            pages: vec![],
            ..prov()
        };
        let all = vec![None, Some(bare), Some(prov())];
        let listed = scopes(&all, &|_| None);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, 2);
        assert_eq!(scope_label(listed[0].0), "TK 3");
    }
}
