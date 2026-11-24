//! Tests that press keys as Windows binds them, on any machine.
//!
//! `scripts/check.sh wording` spells the app's text as Windows does, but the
//! bindings stay this platform's. A key handled on macOS and lost on Windows
//! (Alt+1 in the Publish sheet, taken by the reading layout's Windows key
//! before the sheet saw it) only shows up when the Windows table is bound:
//! `bind_keys_as(Platform::Windows, cx)` binds `keymap::table_for(Windows)`
//! in place of `crate::app::bind_keys`.
//!
//! A ⌘-click has no binding: the click reads the held keys itself
//! (`keymap::secondary_held`, Cmd on a Mac, Ctrl on Windows). So binding a
//! platform's table here also makes this thread's clicks read the keys as
//! that platform does ([`bound_as`]), and a Mac run checks Windows'
//! Ctrl+click too.

use std::cell::Cell;

use gpui_kit::{App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate};

use crate::keymap;
use crate::platform_text::Platform;

/// Bind the key table as `p` binds it (after `gpui_kit::init`, as the app
/// does). Windows' keyboard mapper only respells Shift+digit keys for the
/// layout; the dummy one reads every key as written, which is what the
/// tests press.
pub(crate) fn bind_keys_as(p: Platform, cx: &mut App) {
    BOUND.with(|b| b.set(Some(p)));
    let mut out = Vec::new();
    for k in keymap::table_for(p) {
        let Some(action) = k.new_action.filter(|_| !k.key.is_empty()) else {
            continue;
        };
        let predicate = k.scope.context().map(|c| {
            KeyBindingContextPredicate::parse(c)
                .unwrap_or_else(|e| panic!("context {c}: {e}"))
                .into()
        });
        out.push(
            KeyBinding::load(
                k.key,
                action(),
                predicate,
                false,
                None,
                &DummyKeyboardMapper,
            )
            .unwrap_or_else(|e| panic!("key {}: {e}", k.key)),
        );
    }
    cx.bind_keys(out);
}

thread_local! {
    /// The platform whose table [`bind_keys_as`] bound on this thread (a
    /// GPUI test runs its app, clicks included, on the test's thread).
    static BOUND: Cell<Option<Platform>> = const { Cell::new(None) };
}

/// The platform whose keys this thread's test bound with [`bind_keys_as`]:
/// its clicks read the held keys as that platform does. `None` when the
/// test bound the app's own table (`crate::app::bind_keys`).
pub(crate) fn bound_as() -> Option<Platform> {
    BOUND.with(Cell::get)
}

#[gpui_kit::test]
fn binding_a_platforms_keys_reads_its_clicks_too(cx: &mut gpui_kit::TestAppContext) {
    for p in [Platform::Windows, Platform::Mac] {
        cx.update(|cx| bind_keys_as(p, cx));
        assert_eq!(bound_as(), Some(p));
    }
}
