//! Burrow's built-in themes, resolved by blyg-core's own theme engine
//! (`blyg_core::config::theme`: layers, inheritance, derived colours) and
//! handed to the page as CSS custom properties (`--c-bg`, `--c-ink`, …).
//! Ornaments (textures, sparklines, glyph markers) are GPUI drawing and
//! aren't drawn here.

use blyg_core::config::theme::{COLORS, Registry};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct WebTheme {
    pub id: String,
    pub name: String,
    pub family: &'static str,
    pub dark: bool,
    /// `[("--c-bg", "#fffff8"), …]`, every colour the theme defines or derives.
    pub vars: Vec<(String, String)>,
    pub font_writing: Option<String>,
    pub font_ui: Option<String>,
    pub radius: f32,
}

/// `#rrggbbaa` as CSS takes it (`#rrggbb` when opaque).
fn css_hex(rgba: u32) -> String {
    if rgba & 0xff == 0xff {
        format!("#{:06x}", rgba >> 8)
    } else {
        format!("#{rgba:08x}")
    }
}

pub fn all() -> Vec<WebTheme> {
    let reg = Registry::builtin();
    blyg_core::config::theme::BUILTIN
        .iter()
        .filter_map(|(id, _)| reg.resolve(id))
        .map(|t| WebTheme {
            vars: COLORS
                .iter()
                .map(|c| (format!("--c-{}", c.name), css_hex(t.color(c.name).0)))
                .collect(),
            family: t.family.label(),
            dark: t.is_dark(),
            font_writing: t.font_writing.clone(),
            font_ui: t.font_ui.clone(),
            radius: t.radius,
            name: t.name.clone(),
            id: t.id.clone(),
        })
        .collect()
}
