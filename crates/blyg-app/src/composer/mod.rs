//! Writing aids shared by the main editor and quick capture: @-mention
//! autocomplete and spellcheck. The pure logic lives in `mention` and
//! `spell`; `assist` is the GPUI part a host view embeds.

pub mod assist;
pub mod mention;
pub mod picker;
pub mod spell;
#[cfg(target_os = "macos")]
pub mod spell_mac;
pub mod text;

use std::sync::Arc;

use gpui_kit::{App, Global};

pub use assist::{Assist, AssistKey};
use spell::{Cache, SpellEngine};

gpui_kit::actions!(blygger, [ToggleSpellcheck]);

/// The spell checker, whether it's on (`spellcheck`), and the results cache
/// every editor shares.
pub struct SpellService {
    pub engine: Option<Arc<dyn SpellEngine>>,
    pub enabled: bool,
    pub cache: Cache,
    /// --- dictionary --- the blyg's dictionary (studio 0.69): its words
    /// are never underlined. Empty until read, or on a blyg without one.
    pub accepted: blyg_core::dictionary::Accepted,
    /// Where a learned word goes besides the system's list (the main
    /// window sends it to the blyg's dictionary).
    pub on_learn: Option<OnLearn>,
}

impl Global for SpellService {}

/// Install the spell service. `engine` is `None` where there's no system
/// checker (spellcheck then does nothing).
pub fn init(engine: Option<Arc<dyn SpellEngine>>, enabled: bool, cx: &mut App) {
    cx.set_global(SpellService {
        engine,
        enabled,
        cache: Cache::default(),
        accepted: Default::default(),
        on_learn: None,
    });
}

/// --- dictionary --- The blyg's dictionary as last read: every editor
/// stops underlining its words at its next check.
pub fn set_accepted(accepted: blyg_core::dictionary::Accepted, cx: &mut App) {
    if !cx.has_global::<SpellService>() {
        init(None, false, cx);
    }
    cx.global_mut::<SpellService>().accepted = accepted;
}

/// Where a learned word goes besides the system's list.
/// Where a learned word goes besides the system's list (the main window's
/// `dictionary_learn`, which says when the blyg refuses it).
pub type OnLearn = Arc<dyn Fn(String, &mut App)>;

pub fn set_on_learn(f: OnLearn, cx: &mut App) {
    if !cx.has_global::<SpellService>() {
        init(None, false, cx);
    }
    cx.global_mut::<SpellService>().on_learn = Some(f);
}

/// "Learn Spelling": accepted at once here, and sent on (to the blyg).
fn learned(word: &str, cx: &mut App) {
    let s = cx.global_mut::<SpellService>();
    s.accepted.add(word);
    if let Some(f) = s.on_learn.clone() {
        f(word.to_string(), cx);
    }
}

/// --- dictionary --- A word taken out of the blyg's dictionary: out of
/// the system's learned list too, so it's flagged again.
pub fn unlearned(word: &str, cx: &mut App) {
    if let Some(engine) = cx
        .try_global::<SpellService>()
        .and_then(|s| s.engine.clone())
    {
        engine.unlearn(word);
    }
    if cx.has_global::<SpellService>() {
        // Lines checked with it learned are checked again.
        cx.global_mut::<SpellService>().cache = Cache::default();
    }
}

/// The system engine for this platform.
pub fn system_engine() -> Option<Arc<dyn SpellEngine>> {
    #[cfg(target_os = "macos")]
    {
        Some(Arc::new(spell_mac::MacSpell::new()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

pub fn spellcheck_enabled(cx: &App) -> bool {
    cx.try_global::<SpellService>().is_some_and(|s| s.enabled)
}

/// Turn checking on or off everywhere (the config file is the caller's).
pub fn set_spellcheck(on: bool, cx: &mut App) {
    if let Some(s) = cx.try_global::<SpellService>()
        && s.enabled == on
    {
        return;
    }
    if !cx.has_global::<SpellService>() {
        init(None, on, cx);
    }
    cx.global_mut::<SpellService>().enabled = on;
}

/// Edit › Spelling › Check Spelling While Typing: flip it, remember it in
/// the config file, and re-check the menu item.
pub fn toggle_spellcheck(cx: &mut App) -> Result<bool, String> {
    let on = !spellcheck_enabled(cx);
    set_spellcheck(on, cx);
    crate::refresh_menus(cx);
    let change = blyg_core::config::edit::Change::Set(on.to_string());
    crate::settings::write(&[("spellcheck", change)], cx).map(|()| on)
}
