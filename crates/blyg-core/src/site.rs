//! --- site --- The blyg's public site as Studio sets it up: the reading
//! theme (a preset or a custom palette with an optional dark companion),
//! the typeface, the front page, the posts path, the home-page order, the
//! links menus, the avatar, `authorUrl` / `siteUrl`, collection (hopper)
//! feeds, the studio's update alerts and its extension toggles.
//!
//! These are `GET/PATCH /api/settings` keys that `crate::Settings` (title,
//! name, bio, links, time zone, writing defaults) doesn't carry. A field the
//! server doesn't report reads `None` and is never sent, so a studio that
//! predates a key is never asked to store it. A save sends only what
//! changed (`SiteConfig::patch`).
//!
//! The presets, fonts, path rules and contrast checks mirror
//! blygger-studio's `src/themes.ts`, `src/fonts.ts` and `src/front-page.ts`
//! (0.56).

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::model::AuthorLink;

/// The colour slots of a palette, in Studio's order (`THEME_COLORS`).
pub const THEME_COLORS: [&str; 8] = [
    "page", "paper", "ink", "inkSoft", "rule", "pencil", "genBg", "genRule",
];

/// What each colour slot is for, in words (the editor's labels).
pub const THEME_COLOR_LABELS: [&str; 8] = [
    "Page (behind the sheet)",
    "Paper (the sheet)",
    "Ink (text)",
    "Soft ink (dates, bylines)",
    "Rule (lines)",
    "Pencil (links, accents)",
    "Generated text: tint",
    "Generated text: outline",
];

/// The theme id of a custom palette.
pub const CUSTOM_THEME_ID: &str = "custom";
/// A custom look's name: one line, at most this many characters.
pub const THEME_NAME_MAX: usize = 40;
/// The default typeface (the stylesheet's own stack).
pub const DEFAULT_FONT: &str = "book";
/// The default posts path.
pub const DEFAULT_POSTS_PATH: &str = "posts";
/// The links section's lists each hold at most this many.
pub const MAX_LINKS: usize = 30;
/// Where Studio's release notes are.
pub const STUDIO_RELEASES_URL: &str = "https://github.com/blygger/blygger-studio/releases";

/// A palette: the theme's eight colours (`#rrggbb`) and whether it's dark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTheme {
    pub dark: bool,
    pub page: String,
    pub paper: String,
    pub ink: String,
    pub ink_soft: String,
    pub rule: String,
    pub pencil: String,
    pub gen_bg: String,
    pub gen_rule: String,
}

impl CustomTheme {
    /// A colour by its slot (`THEME_COLORS`).
    pub fn get(&self, slot: &str) -> &str {
        match slot {
            "page" => &self.page,
            "paper" => &self.paper,
            "ink" => &self.ink,
            "inkSoft" => &self.ink_soft,
            "rule" => &self.rule,
            "pencil" => &self.pencil,
            "genBg" => &self.gen_bg,
            "genRule" => &self.gen_rule,
            _ => "",
        }
    }

    /// Set a colour by its slot.
    pub fn set(&mut self, slot: &str, value: String) {
        match slot {
            "page" => self.page = value,
            "paper" => self.paper = value,
            "ink" => self.ink = value,
            "inkSoft" => self.ink_soft = value,
            "rule" => self.rule = value,
            "pencil" => self.pencil = value,
            "genBg" => self.gen_bg = value,
            "genRule" => self.gen_rule = value,
            _ => {}
        }
    }

    /// The first slot that isn't a `#rrggbb` colour, as Studio says it.
    pub fn error(&self) -> Option<String> {
        THEME_COLORS
            .iter()
            .find(|s| !is_hex(self.get(s)))
            .map(|s| format!("{s} must be a #rrggbb colour"))
    }

    /// Lower-cased colours, as Studio stores them.
    pub fn normalized(&self) -> CustomTheme {
        let mut t = self.clone();
        for s in THEME_COLORS {
            let v = t.get(s).trim().to_ascii_lowercase();
            t.set(s, v);
        }
        t
    }
}

/// One of Studio's preset themes (`THEMES`).
#[derive(Debug, Clone, Copy)]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub dark: bool,
    /// page, paper, ink, inkSoft, rule, pencil, genBg, genRule.
    pub colors: [&'static str; 8],
}

impl Preset {
    pub fn palette(&self) -> CustomTheme {
        let c = self.colors;
        CustomTheme {
            dark: self.dark,
            page: c[0].into(),
            paper: c[1].into(),
            ink: c[2].into(),
            ink_soft: c[3].into(),
            rule: c[4].into(),
            pencil: c[5].into(),
            gen_bg: c[6].into(),
            gen_rule: c[7].into(),
        }
    }
}

/// Studio 0.56's presets, in its order. `auto` (the light/dark defaults)
/// isn't one: it's the absence of a choice.
pub const PRESETS: [Preset; 6] = [
    Preset {
        id: "paper",
        label: "Paper",
        dark: false,
        colors: [
            "#fafbfb", "#fafbfb", "#1b2426", "#5c686b", "#dde3e5", "#23608c", "#eef3f7", "#c9d8e4",
        ],
    },
    Preset {
        id: "cream",
        label: "Cream",
        dark: false,
        colors: [
            "#e9e2d2", "#f7f2e7", "#33312c", "#6d675c", "#ddd5c4", "#8a5a2b", "#f0e7d4", "#d8c39e",
        ],
    },
    Preset {
        id: "slate",
        label: "Slate",
        dark: false,
        colors: [
            "#2f3538", "#f5f7f7", "#1b2426", "#5c686b", "#dde3e5", "#23608c", "#ebf1f5", "#c6d6e2",
        ],
    },
    Preset {
        id: "solarized-light",
        label: "Solarized Light",
        dark: false,
        colors: [
            "#eee8d5", "#fdf6e3", "#073642", "#657b83", "#e3dcc4", "#268bd2", "#f3ecd6", "#d8cca5",
        ],
    },
    Preset {
        id: "solarized-dark",
        label: "Solarized Dark",
        dark: true,
        colors: [
            "#00212b", "#002b36", "#eee8d5", "#93a1a1", "#0c4553", "#6cb6e0", "#073642", "#1d5566",
        ],
    },
    Preset {
        id: "nord",
        label: "Nord",
        dark: true,
        colors: [
            "#242933", "#2e3440", "#e5e9f0", "#a5aec0", "#3e4757", "#88c0d0", "#3b4252", "#4c566a",
        ],
    },
];

/// The light and dark defaults `auto` follows (Studio's base stylesheet).
pub const AUTO_LIGHT: [&str; 8] = PRESETS[0].colors;
pub const AUTO_DARK: [&str; 8] = [
    "#14191a", "#14191a", "#e3e7e7", "#95a2a5", "#2b3436", "#8cc0e4", "#1a2429", "#2f4552",
];

pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// A preset's palette as a custom theme's starting point (Paper for
/// anything else, as Studio does).
pub fn custom_from_preset(id: &str) -> CustomTheme {
    preset(id).unwrap_or(&PRESETS[0]).palette()
}

/// One of Studio's typefaces (`FONTS`): a CSS stack of fonts already on
/// the reader's device, and the faces Burrow previews it with here.
#[derive(Debug, Clone, Copy)]
pub struct Font {
    pub id: &'static str,
    pub label: &'static str,
    /// The face to preview with on macOS and on Windows.
    pub mac: &'static str,
    pub windows: &'static str,
}

pub const FONTS: [Font; 8] = [
    Font {
        id: "book",
        label: "Book (default)",
        mac: "Iowan Old Style",
        windows: "Palatino Linotype",
    },
    Font {
        id: "transitional",
        label: "Transitional",
        mac: "Charter",
        windows: "Cambria",
    },
    Font {
        id: "humanist",
        label: "Humanist sans",
        mac: "Seravek",
        windows: "Calibri",
    },
    Font {
        id: "geometric",
        label: "Geometric sans",
        mac: "Avenir",
        windows: "Corbel",
    },
    Font {
        id: "system",
        label: "System sans",
        mac: ".SystemUIFont",
        windows: "Segoe UI",
    },
    Font {
        id: "rounded",
        label: "Rounded",
        mac: "Arial Rounded MT Bold",
        windows: "Calibri",
    },
    Font {
        id: "slab",
        label: "Slab serif",
        mac: "Rockwell",
        windows: "Rockwell",
    },
    Font {
        id: "mono",
        label: "Monospace",
        mac: "Menlo",
        windows: "Consolas",
    },
];

pub fn font(id: &str) -> Option<&'static Font> {
    FONTS.iter().find(|f| f.id == id)
}

/// A home-page order the blyg offers (`feedOrders`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedOrder {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
}

/// The core orders, for a studio that doesn't list `feedOrders`.
pub fn core_feed_orders() -> Vec<FeedOrder> {
    vec![
        FeedOrder {
            id: "pace-layered".into(),
            title: "Pace-layered".into(),
            description: "Items sink as new ones arrive, more slowly the more they have been pinned, and climb one place with each new version. What keeps mattering stays near the top.".into(),
        },
        FeedOrder {
            id: "reverse-chronological".into(),
            title: "Newest first".into(),
            description: "Items in the order they were last published, most recent at the top.".into(),
        },
    ]
}

/// The default home-page order.
pub const DEFAULT_FEED_ORDER: &str = "pace-layered";

/// Where the links section sits (`linksPosition`).
pub const LINKS_POSITIONS: [(&str, &str); 3] = [
    ("left", "On the left (default)"),
    ("right", "On the right"),
    ("bottom", "Below the content"),
];

/// A studio extension and whether it's on (from the blyg's capabilities).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerExtension {
    pub name: String,
    /// Compiled into this studio's build: only these can be turned on.
    pub compiled: bool,
    pub enabled: bool,
}

/// Studio's label and description for an extension it ships (its
/// `extensions/*/ui`), or the name and nothing.
pub fn extension_label(name: &str) -> (&str, &'static str) {
    match name {
        "inspect" => (
            "Inspect",
            "Adds “inspect” to each reading entry’s ⋯ sheet in Studio: the record it holds for it, and its JSON.",
        ),
        "reading-time" => (
            "Reading time",
            "Shows an estimated reading time and word count at the end of each reading entry’s byline in Studio.",
        ),
        "lineage-glyph" => (
            "Lineage glyph",
            "Where each reading entry came from, and what each action makes. Burrow draws its own glyph either way.",
        ),
        "word-count" => (
            "Word count",
            "Word counts for reading entries, served at /api/ext/word-count/counts.",
        ),
        "example" => (
            "Example extension",
            "A marker on each reading byline, an “example” row and a page under More: for trying the extension slots.",
        ),
        "feed-example" => (
            "Pinned-first home page",
            "Adds a home page order with pinned items at the top; choose it under Home page once enabled and saved.",
        ),
        _ => (name, ""),
    }
}

/// What the blyg's studio knows about its own updates (`GET
/// /api/update-state`, and the version it says it runs).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StudioUpdate {
    /// The studio's own version (`blyg.json`'s `generator`), when known.
    pub current: Option<String>,
    /// The newest release its last check saw; `""` before any check.
    pub latest: String,
    /// When it last checked (ISO); `""` for never.
    pub checked_at: String,
}

impl StudioUpdate {
    /// A newer release than the running studio exists. Ahead or unknown
    /// isn't behind (Studio's `isBehind`).
    pub fn behind(&self) -> bool {
        match (
            self.current.as_deref().and_then(parse_version),
            parse_version(&self.latest),
        ) {
            (Some(c), Some(l)) => c < l,
            _ => false,
        }
    }
}

/// `1.2.3` (with anything around it) → (1, 2, 3).
pub fn parse_version(raw: &str) -> Option<(u64, u64, u64)> {
    let s = raw.trim();
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let mut parts = s[start..]
        .split(|c: char| !c.is_ascii_digit())
        .map(|p| p.parse::<u64>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

/// The version in a `generator` such as `blygger-studio/0.56.0`.
pub fn generator_version(generator: &str) -> Option<String> {
    let v = generator.rsplit('/').next()?.trim();
    parse_version(v).map(|(a, b, c)| format!("{a}.{b}.{c}"))
}

/// Settings keys that are present but `null` stay `Some(None)`.
fn nullable<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// The site settings Burrow's `Settings` doesn't carry. `None`: the
/// server doesn't report it (and it's never sent).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteConfig {
    // Profile
    #[serde(default)]
    pub author_url: Option<String>,
    #[serde(default)]
    pub site_url: Option<String>,
    #[serde(default)]
    pub avatar_media_id: Option<String>,
    // Appearance
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub custom_theme: Option<Option<CustomTheme>>,
    #[serde(default, deserialize_with = "nullable")]
    pub custom_theme_dark: Option<Option<CustomTheme>>,
    #[serde(default)]
    pub custom_theme_name: Option<String>,
    #[serde(default)]
    pub font: Option<String>,
    // Home page
    #[serde(default)]
    pub feed_order: Option<String>,
    #[serde(default, skip_serializing)]
    pub feed_orders: Option<Vec<FeedOrder>>,
    #[serde(default)]
    pub front_page: Option<String>,
    #[serde(default)]
    pub posts_path: Option<String>,
    // Links
    #[serde(default)]
    pub links_position: Option<String>,
    #[serde(default)]
    pub links_threads: Option<Vec<String>>,
    #[serde(default)]
    pub links_external: Option<Vec<AuthorLink>>,
    #[serde(default)]
    pub links_collections: Option<bool>,
    #[serde(default)]
    pub links_opml: Option<bool>,
    #[serde(default)]
    pub links_blygger_org: Option<bool>,
    #[serde(default)]
    pub links_blygger_com: Option<bool>,
    // Network
    #[serde(default)]
    pub hopper_feeds: Option<bool>,
    // Updates
    #[serde(default)]
    pub update_check: Option<bool>,
    #[serde(default)]
    pub update_feed_url: Option<String>,
    #[serde(default)]
    pub update_notice_ack: Option<bool>,
    // Extensions
    #[serde(default)]
    pub extensions: Option<Vec<String>>,
    /// The studio's extensions, from its capabilities (never sent).
    #[serde(skip)]
    pub server_extensions: Vec<ServerExtension>,
    /// Its update state, when it serves one (never sent).
    #[serde(skip)]
    pub update: Option<StudioUpdate>,
}

impl SiteConfig {
    /// The `PATCH /api/settings` body: every field that differs from
    /// `base` (as last read), and nothing else. Read-only and unreported
    /// fields never go.
    pub fn patch(&self, base: &SiteConfig) -> Map<String, Value> {
        let now = serde_json::to_value(self).unwrap_or(Value::Null);
        let was = serde_json::to_value(base).unwrap_or(Value::Null);
        let (Value::Object(now), Value::Object(was)) = (now, was) else {
            return Map::new();
        };
        let mut out = Map::new();
        for (k, v) in now {
            if was.get(&k) != Some(&v) {
                out.insert(k, v);
            }
        }
        out
    }

    /// Why this can't be saved as it stands (checked before it's sent, as
    /// Studio checks it), or `None`.
    pub fn error(&self) -> Option<String> {
        if let Some(p) = self.posts_path.as_deref()
            && let Some(e) = posts_path_error(p)
        {
            return Some(e);
        }
        if let Some(n) = self.custom_theme_name.as_deref()
            && let Some(e) = theme_name_error(n)
        {
            return Some(e);
        }
        let light = self.custom_theme.clone().flatten();
        let dark = self.custom_theme_dark.clone().flatten();
        if let Some(e) = light.as_ref().and_then(CustomTheme::error) {
            return Some(format!("Custom theme: {e}"));
        }
        if let Some(e) = dark.as_ref().and_then(CustomTheme::error) {
            return Some(format!("Dark companion: {e}"));
        }
        if self.theme.as_deref() == Some(CUSTOM_THEME_ID) && light.is_none() {
            return Some("The custom theme has no colours yet".into());
        }
        if let Some(e) = pair_error(light.as_ref(), dark.as_ref()) {
            return Some(e);
        }
        if self
            .links_threads
            .as_ref()
            .is_some_and(|t| t.len() > MAX_LINKS)
        {
            return Some(format!("The main menu lists at most {MAX_LINKS} threads"));
        }
        if self
            .links_external
            .as_ref()
            .is_some_and(|t| t.len() > MAX_LINKS)
        {
            return Some(format!("The main menu lists at most {MAX_LINKS} links"));
        }
        None
    }
}

/// A `#rrggbb` colour (either case).
pub fn is_hex(s: &str) -> bool {
    let s = s.trim();
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// `#rrggbb` → (r, g, b).
pub fn hex_rgb(s: &str) -> Option<(u8, u8, u8)> {
    if !is_hex(s) {
        return None;
    }
    let n = u32::from_str_radix(&s.trim()[1..], 16).ok()?;
    Some(((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

/// WCAG relative luminance.
pub fn luminance(hex: &str) -> f64 {
    let Some((r, g, b)) = hex_rgb(hex) else {
        return 0.0;
    };
    let ch = |x: u8| {
        let x = x as f64 / 255.0;
        if x <= 0.03928 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)
}

/// WCAG contrast ratio (1–21).
pub fn contrast_ratio(a: &str, b: &str) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

/// Studio's advisory contrast checks: (foreground, background, label).
pub const CONTRAST_CHECKS: [(&str, &str, &str); 4] = [
    ("ink", "paper", "Text on paper"),
    ("inkSoft", "paper", "Secondary text on paper"),
    ("pencil", "paper", "Links on paper"),
    ("ink", "genBg", "Text in generated highlights"),
];

/// The checks a palette fails (WCAG AA, 4.5:1): (label, ratio). Advisory,
/// as in Studio: it warns, it doesn't refuse.
pub fn contrast_warnings(t: &CustomTheme) -> Vec<(&'static str, f64)> {
    CONTRAST_CHECKS
        .iter()
        .filter(|(fg, bg, _)| is_hex(t.get(fg)) && is_hex(t.get(bg)))
        .map(|(fg, bg, label)| (*label, contrast_ratio(t.get(fg), t.get(bg))))
        .filter(|(_, r)| *r < 4.5)
        .collect()
}

/// A light palette and its dark companion must say which they are.
pub fn pair_error(light: Option<&CustomTheme>, dark: Option<&CustomTheme>) -> Option<String> {
    let dark = dark?;
    let Some(light) = light else {
        return Some("A dark companion needs a custom theme to pair with".into());
    };
    if light.dark {
        return Some("With a dark companion, the custom theme must be a light palette".into());
    }
    if !dark.dark {
        return Some("The dark companion must be a dark palette".into());
    }
    None
}

/// Names the blyg already uses under its mount (Studio's `RESERVED_PATHS`).
pub const RESERVED_PATHS: [&str; 14] = [
    "api",
    "archive",
    "atom",
    "auth",
    "f",
    "feed",
    "h",
    "items",
    "media",
    "mcp",
    "rss",
    "studio",
    "t",
    "webmention",
];

/// Why `path` can't be the posts page's address, or `None`.
pub fn posts_path_error(path: &str) -> Option<String> {
    let ok = !path.is_empty()
        && path.len() <= 40
        && path
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && path
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !ok {
        return Some(
            "The posts path is 1–40 lowercase letters, digits or hyphens, starting with a letter or digit"
                .into(),
        );
    }
    if RESERVED_PATHS.contains(&path) {
        return Some(format!(
            "The posts path can't be “{path}”: the blyg already uses it"
        ));
    }
    None
}

/// Why a custom look's name won't do, or `None`.
pub fn theme_name_error(name: &str) -> Option<String> {
    if name.chars().count() > THEME_NAME_MAX {
        return Some(format!(
            "The theme's name is at most {THEME_NAME_MAX} characters"
        ));
    }
    if name.chars().any(|c| c.is_control()) {
        return Some("The theme's name is one line of plain text".into());
    }
    None
}

/// The palette public pages show for a theme id (`None`: `auto`, the
/// defaults, which follow the reader's light/dark preference).
pub fn resolve(cfg: &SiteConfig, prefers_dark: bool) -> Option<CustomTheme> {
    let id = cfg.theme.as_deref().unwrap_or("auto");
    if id == CUSTOM_THEME_ID {
        let light = cfg.custom_theme.clone().flatten()?;
        let dark = cfg.custom_theme_dark.clone().flatten();
        return Some(match dark {
            Some(d) if prefers_dark => d,
            _ => light,
        });
    }
    preset(id).map(Preset::palette)
}

/// `auto`'s palette for a light or dark reader.
pub fn auto_palette(dark: bool) -> CustomTheme {
    Preset {
        id: "auto",
        label: "Auto",
        dark,
        colors: if dark { AUTO_DARK } else { AUTO_LIGHT },
    }
    .palette()
}

/// The capabilities' `extensions` list.
pub fn parse_extensions(caps: &Value) -> Vec<ServerExtension> {
    caps.get("extensions")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    Some(ServerExtension {
                        name: e.get("name")?.as_str()?.to_string(),
                        compiled: e.get("compiled").and_then(Value::as_bool).unwrap_or(false),
                        enabled: e.get("enabled").and_then(Value::as_bool).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `GET /api/update-state` (`{updateCheckedAt?, updateLatestSeen?}`).
pub fn parse_update_state(v: &Value, current: Option<String>) -> StudioUpdate {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    StudioUpdate {
        current,
        latest: s("updateLatestSeen"),
        checked_at: s("updateCheckedAt"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "siteTitle": "A blyg", "theme": "auto", "customTheme": null,
            "customThemeDark": null, "customThemeName": "", "font": "book",
            "authorUrl": "", "siteUrl": "", "avatarMediaId": "",
            "feedOrder": "pace-layered", "frontPage": "", "postsPath": "posts",
            "linksPosition": "left", "linksThreads": [], "linksExternal": [],
            "linksCollections": true, "linksOpml": true, "linksBlyggerOrg": true,
            "linksBlyggerCom": false, "hopperFeeds": false, "updateCheck": true,
            "updateFeedUrl": "", "updateNoticeAck": false, "extensions": [],
            "feedOrders": [{"id": "pace-layered", "title": "Pace", "description": "d"}],
        })
    }

    #[test]
    fn reads_the_studio_shape_and_sends_only_changes() {
        let base: SiteConfig = serde_json::from_value(sample()).unwrap();
        assert_eq!(
            base.custom_theme,
            Some(None),
            "null is reported, not absent"
        );
        assert_eq!(base.feed_orders.as_ref().unwrap().len(), 1);
        assert!(base.patch(&base).is_empty());
        let mut next = base.clone();
        next.theme = Some("custom".into());
        next.custom_theme = Some(Some(custom_from_preset("cream")));
        next.hopper_feeds = Some(true);
        let p = next.patch(&base);
        assert_eq!(p.len(), 3, "{p:?}");
        assert_eq!(p["theme"], json!("custom"));
        assert_eq!(p["customTheme"]["inkSoft"], json!("#6d675c"));
        assert_eq!(p["hopperFeeds"], json!(true));
        // Back to no custom theme: null goes.
        let mut cleared = next.clone();
        cleared.custom_theme = Some(None);
        assert_eq!(cleared.patch(&next)["customTheme"], Value::Null);
    }

    #[test]
    fn an_older_studio_is_never_sent_what_it_does_not_report() {
        let base: SiteConfig = serde_json::from_value(json!({"theme": "auto"})).unwrap();
        assert_eq!(base.hopper_feeds, None);
        assert_eq!(base.custom_theme, None);
        assert!(base.clone().patch(&base).is_empty());
    }

    #[test]
    fn posts_paths_follow_studio_rules() {
        assert_eq!(posts_path_error("posts"), None);
        assert_eq!(posts_path_error("writing-2"), None);
        assert!(posts_path_error("").is_some());
        assert!(posts_path_error("-a").is_some());
        assert!(posts_path_error("Posts").is_some());
        assert!(posts_path_error("feed").unwrap().contains("already uses"));
        assert!(posts_path_error(&"a".repeat(41)).is_some());
    }

    #[test]
    fn custom_themes_are_checked_like_studio() {
        let mut light = custom_from_preset("paper");
        assert_eq!(light.error(), None);
        light.ink = "red".into();
        assert_eq!(light.error().unwrap(), "ink must be a #rrggbb colour");
        let light = custom_from_preset("paper");
        let dark = custom_from_preset("nord");
        assert_eq!(pair_error(Some(&light), Some(&dark)), None);
        assert!(pair_error(Some(&dark), Some(&dark)).is_some());
        assert!(pair_error(Some(&light), Some(&light)).is_some());
        assert!(pair_error(None, Some(&dark)).is_some());
        assert!(theme_name_error(&"x".repeat(41)).is_some());
        assert!(theme_name_error("Pink\nQuill").is_some());
        assert_eq!(theme_name_error("Pink Quill"), None);
    }

    #[test]
    fn contrast_matches_wcag() {
        assert!((contrast_ratio("#000000", "#ffffff") - 21.0).abs() < 0.01);
        assert!(contrast_warnings(&custom_from_preset("paper")).is_empty());
        let mut t = custom_from_preset("paper");
        t.ink = "#eeeeee".into();
        let w = contrast_warnings(&t);
        assert!(w.iter().any(|(l, _)| *l == "Text on paper"));
    }

    #[test]
    fn update_state_compares_versions() {
        let u = StudioUpdate {
            current: generator_version("blygger-studio/0.56.0"),
            latest: "0.57.1".into(),
            checked_at: String::new(),
        };
        assert_eq!(u.current.as_deref(), Some("0.56.0"));
        assert!(u.behind());
        let ahead = StudioUpdate {
            latest: "0.55.9".into(),
            ..u.clone()
        };
        assert!(!ahead.behind());
        let never = StudioUpdate {
            latest: String::new(),
            ..u
        };
        assert!(!never.behind());
        assert_eq!(parse_version("v1.2.3-rc1"), Some((1, 2, 3)));
    }

    #[test]
    fn capabilities_list_extensions() {
        let v = json!({"extensions": [
            {"name": "inspect", "declared": false, "compiled": true, "enabled": false, "granted": true},
            {"name": "word-count", "declared": true, "compiled": false, "enabled": false, "granted": true},
        ]});
        let e = parse_extensions(&v);
        assert_eq!(e.len(), 2);
        assert!(e[0].compiled && !e[0].enabled);
        assert_eq!(extension_label("inspect").0, "Inspect");
    }
}
