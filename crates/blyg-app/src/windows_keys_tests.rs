//! Tests that press keys as Windows binds them, on any machine.
//!
//! `scripts/check.sh wording` spells the app's text as Windows does, but the
//! bindings stay this platform's. A key handled on macOS and lost on Windows
//! (Alt+1 in the Publish sheet, taken by the reading layout's Windows key
//! before the sheet saw it) only shows up when the Windows table is bound:
//! `bind_keys_as(Platform::Windows, cx)` binds `keymap::table_for(Windows)`
//! in place of `crate::app::bind_keys`.

use gpui_kit::{App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate};

use crate::keymap;
use crate::platform_text::Platform;

/// Bind the key table as `p` binds it (after `gpui_kit::init`, as the app
/// does). Windows' keyboard mapper only respells Shift+digit keys for the
/// layout; the dummy one reads every key as written, which is what the
/// tests press.
pub(crate) fn bind_keys_as(p: Platform, cx: &mut App) {
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
