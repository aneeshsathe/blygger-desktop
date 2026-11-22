//! --- dictionary --- Blyg › Dictionary…: the words your blyg's spellcheck
//! accepts (studio 0.69, contract 6.2.0; `blyg_core::dictionary`), shared
//! with Studio and every other client. Your own words can be added and
//! taken out; sets an extension installed are listed by name.
//!
//! Burrow's spellcheck (the system's, `composer::spell_mac`) accepts every
//! word in it, and Learn Spelling in the editor adds the word here as well
//! as to the system's list, as Studio's "add to dictionary" does. The
//! dictionary is read when the window opens, when this sheet opens, and
//! again after a while on Reading's refreshes ([`READ_EVERY`]). A word
//! learned or edited offline is kept on this computer
//! (`blyg_core::dictionary::Pending`) and sent by the next read; one the
//! blyg refuses is said in a toast. A failed read isn't counted as a read.
//!
//! Burrow keeps a record of the words its Learn Spelling added to the
//! system's list (`Backend::dictionary_taught`). When one of those leaves
//! the blyg's words (taken out here, or in Studio), the next read unlearns
//! it on this computer; a word the system learned any other way is never
//! touched. A sign-in that may read the dictionary but not change it (no
//! `putDictionarySource`) learns on this computer only, quietly, and the
//! sheet says it can't add words.
//!
//! Keys: type a word and ⏎ adds it; ↑/↓ choose one of your words, ⌘⌫
//! takes it out; esc closes. A blyg without a dictionary (its capabilities
//! don't list `getDictionary`) says so.

use std::time::{Duration, Instant};

use blyg_core::Backend;
use blyg_core::dictionary::{Accepted, Dictionary, Edit, OWNER, word_problem};
use std::sync::Arc;

use super::*;

#[cfg(test)]
#[path = "dictionary_tests.rs"]
mod dictionary_tests;

gpui_kit::actions!(
    blygger,
    [
        /// Blyg › Dictionary…
        ShowDictionary
    ]
);

/// How long a read of the dictionary stands before Reading's refreshes
/// read it again.
pub const READ_EVERY: Duration = Duration::from_secs(300);

/// What the sheet says to a sign-in that can't change the dictionary.
pub const READ_ONLY_NOTE: &str = "This sign-in can read your blyg's dictionary but can't add words \
     to it. Learn Spelling keeps a word on this computer only.";

/// After a read (or a write) of `d`: the words Burrow taught this
/// computer that have left the blyg's words, forgotten here; the caller
/// unlearns them ([`unlearn_all`]).
fn settle(backend: &Arc<dyn Backend>, d: &Dictionary) -> Vec<String> {
    let taught = backend.dictionary_taught().unwrap_or_default();
    let gone = blyg_core::dictionary::left(&taught, d);
    for w in &gone {
        let _ = backend.dictionary_teach(w, false);
    }
    gone
}

/// A read of the dictionary, and the taught words it settles ([`settle`]).
fn read_settled(
    backend: &Arc<dyn Backend>,
) -> blyg_core::Result<(Option<Dictionary>, Vec<String>)> {
    let d = backend.dictionary()?;
    let gone = d.as_ref().map(|d| settle(backend, d)).unwrap_or_default();
    Ok((d, gone))
}

/// Out of the system's learned list: words Burrow taught that left the blyg.
fn unlearn_all(gone: &[String], cx: &mut App) {
    for w in gone {
        crate::composer::unlearned(w, cx);
    }
}

/// When the dictionary was last read (per window session).
#[derive(Default)]
struct LastRead(Option<Instant>);

impl Global for LastRead {}

pub enum DictLoad {
    Loading,
    Ready(Dictionary),
    /// The blyg doesn't keep a dictionary.
    Unoffered,
    Failed(String),
}

pub struct DictionarySheet {
    pub focus: FocusHandle,
    pub add: Entity<InputState>,
    pub load: DictLoad,
    /// The selected word among yours.
    pub sel: usize,
    pub busy: bool,
    pub error: Option<String>,
}

impl crate::app::sheet::SheetView for DictionarySheet {
    fn render(
        &self,
        v: &crate::app::MainView,
        _: &crate::app::sheet::Kit,
        cx: &mut Context<crate::app::MainView>,
    ) -> crate::app::sheet::Shown {
        v.render_dictionary_sheet(self, cx).into()
    }

    fn inputs(&self) -> Vec<&Entity<InputState>> {
        vec![&self.add]
    }
}

/// The other sources, in words: "maritime (2 words)".
pub fn other_sources(d: &Dictionary) -> Option<String> {
    let others: Vec<String> = d
        .sources
        .iter()
        .filter(|s| s.source != OWNER)
        .map(|s| {
            let n = s.words.len();
            format!("{} ({n} word{})", s.source, if n == 1 { "" } else { "s" })
        })
        .collect();
    (!others.is_empty()).then(|| format!("Also accepted, installed sets: {}", others.join(", ")))
}

impl MainView {
    pub(super) fn dictionary_actions(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &ShowDictionary, window, cx| this.open_dictionary(window, cx)),
        )
    }

    /// Read the blyg's dictionary for the spellcheck, unless it was read in
    /// the last [`READ_EVERY`] (`force`: read it now). Also routes Learn
    /// Spelling's words to the blyg.
    pub(crate) fn dictionary_refresh(&mut self, force: bool, cx: &mut Context<Self>) {
        let fresh = cx
            .try_global::<LastRead>()
            .and_then(|l| l.0)
            .is_some_and(|t| t.elapsed() < READ_EVERY);
        if fresh && !force {
            return;
        }
        let me = cx.entity().downgrade();
        crate::composer::set_on_learn(
            std::sync::Arc::new(move |word: String, cx: &mut App| {
                // Deferred: the spelling menu answers inside the main
                // window's own update.
                let me = me.clone();
                cx.defer(move |cx| {
                    if let Some(v) = me.upgrade() {
                        v.update(cx, |v, cx| v.dictionary_learn(word, cx));
                    }
                });
            }),
            cx,
        );
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { read_settled(&backend) });
        cx.spawn(async move |_, cx| {
            let read = task.await;
            cx.update(|cx| {
                // Only a read that came back counts: a failed one is tried
                // again at the next refresh.
                if let Ok((d, gone)) = &read {
                    cx.set_global(LastRead(Some(Instant::now())));
                    if let Some(d) = d {
                        crate::composer::set_accepted(Accepted::from(d), cx);
                    }
                    unlearn_all(gone, cx);
                }
            });
        })
        .detach();
    }

    /// Learn Spelling's word, on to the blyg's dictionary off the UI
    /// thread. Offline (or a later try can mend it), the backend keeps it
    /// and the next read sends it, quietly; a refusal is said. A sign-in
    /// that can't change the dictionary sends nothing and says nothing.
    /// A word sent (or waiting to be) is recorded as taught: Burrow's to
    /// unlearn once it leaves the blyg.
    pub(crate) fn dictionary_learn(&mut self, word: String, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let w = word.clone();
        let task = cx.background_spawn(async move {
            let r = backend.edit_dictionary(&Edit::Add(w.clone()));
            let sent = match &r {
                Ok(d) => d.writable,
                Err(e) => blyg_core::dictionary::keeps_queue(e),
            };
            if sent {
                let _ = backend.dictionary_teach(&w, true);
            }
            r
        });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| match r {
                Ok(d) => crate::composer::set_accepted(Accepted::from(&d), cx),
                Err(e) if blyg_core::dictionary::keeps_queue(&e) => {}
                Err(e) => v.show_toast(
                    format!("Your blyg's dictionary didn't take “{word}”"),
                    Some(format!("{e}. It's still learned on this computer.").into()),
                    cx,
                ),
            });
        })
        .detach();
    }

    /// Blyg › Dictionary…: open it (or close it).
    pub(crate) fn open_dictionary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sheet.is::<DictionarySheet>() {
            return self.close_sheet(window, cx);
        }
        let add = cx.new(|cx| InputState::new(window, cx).placeholder("Add a word, then ⏎"));
        self._subs
            .push(cx.subscribe_in(&add, window, |this, _, ev, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.dictionary_add(window, cx);
                }
            }));
        add.update(cx, |e, cx| e.focus(window, cx));
        self.sheet.open(DictionarySheet {
            focus: cx.focus_handle(),
            add,
            load: DictLoad::Loading,
            sel: 0,
            busy: false,
            error: None,
        });
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { read_settled(&backend) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, _, cx| {
                let r = r.map(|(d, gone)| {
                    unlearn_all(&gone, cx);
                    Some(d)
                });
                v.dictionary_loaded(r, cx)
            });
        })
        .detach();
    }

    fn dictionary_sheet(&mut self) -> Option<&mut DictionarySheet> {
        self.sheet.get_mut::<DictionarySheet>()
    }

    /// A read or a write came back.
    fn dictionary_loaded(
        &mut self,
        r: blyg_core::Result<Option<Option<Dictionary>>>,
        cx: &mut Context<Self>,
    ) {
        if let Ok(Some(Some(d))) = &r {
            crate::composer::set_accepted(Accepted::from(d), cx);
        }
        let Some(s) = self.dictionary_sheet() else {
            return;
        };
        s.busy = false;
        match r {
            Ok(Some(Some(d))) => {
                s.sel = s.sel.min(d.owner_words().len().saturating_sub(1));
                s.load = DictLoad::Ready(d);
            }
            Ok(Some(None)) => s.load = DictLoad::Unoffered,
            // A write's error: keep the list, say why.
            Ok(None) => {}
            Err(e) => match s.load {
                DictLoad::Ready(_) => s.error = Some(format!("Couldn't save: {e}")),
                _ => s.load = DictLoad::Failed(e.to_string()),
            },
        }
        cx.notify();
    }

    /// Send an edit; the dictionary as stored comes back.
    fn dictionary_edit(&mut self, edit: Edit, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.dictionary_sheet() else {
            return;
        };
        // A sign-in that can't change it isn't asked to.
        if s.busy || !matches!(&s.load, DictLoad::Ready(d) if d.writable) {
            return;
        }
        s.busy = true;
        s.error = None;
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let r = backend.edit_dictionary(&edit);
            r.map(|d| {
                let gone = settle(&backend, &d);
                (d, gone)
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, _, cx| match r {
                Ok((d, gone)) => {
                    // A word taken out that Burrow taught: flagged again.
                    unlearn_all(&gone, cx);
                    v.dictionary_loaded(Ok(Some(Some(d))), cx)
                }
                Err(e) => {
                    if let Some(s) = v.dictionary_sheet() {
                        s.busy = false;
                        s.error = Some(match e {
                            // Queued: the next read of the dictionary sends it.
                            blyg_core::CoreError::Offline => {
                                "Offline: kept on this computer, and sent when your blyg is back."
                                    .into()
                            }
                            e => format!("Couldn't save: {e}"),
                        });
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// ⏎ in the field: add the typed word.
    pub(crate) fn dictionary_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.dictionary_sheet() else {
            return;
        };
        let word = s.add.read(cx).value().trim().to_string();
        if let Some(p) = word_problem(&word) {
            if !word.is_empty() {
                s.error = Some(format!(
                    "“{word}” {p}: a word is one to 64 characters, with a letter and no space."
                ));
                cx.notify();
            }
            return;
        }
        let add = s.add.clone();
        add.update(cx, |e, cx| e.set_value("", window, cx));
        self.dictionary_edit(Edit::Add(word), window, cx);
    }

    /// ⌘⌫ (or its ×): take one of your words out.
    pub(crate) fn dictionary_remove(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.dictionary_sheet() else {
            return;
        };
        let DictLoad::Ready(d) = &s.load else { return };
        let Some(w) = d.owner_words().get(ix).cloned() else {
            return;
        };
        // Unlearned here too once stored, if Burrow taught it (`settle`).
        self.dictionary_edit(Edit::Remove(w), window, cx);
    }

    fn dictionary_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.dictionary_sheet() else {
            return;
        };
        let n = match &s.load {
            DictLoad::Ready(d) => d.owner_words().len(),
            _ => 0,
        };
        let sel = s.sel;
        let k = &ev.keystroke;
        match k.key.as_str() {
            "escape" => self.close_sheet(window, cx),
            "up" => {
                s.sel = sel.saturating_sub(1);
                cx.notify();
            }
            "down" => {
                s.sel = (sel + 1).min(n.saturating_sub(1));
                cx.notify();
            }
            "backspace" if k.modifiers.platform || k.modifiers.control => {
                self.dictionary_remove(sel, window, cx)
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    pub(super) fn render_dictionary_sheet(
        &self,
        s: &DictionarySheet,
        cx: &mut Context<Self>,
    ) -> (f32, AnyElement) {
        let p = self.palette;
        let theme = &*self.theme;
        let muted = |t: String| {
            div()
                .text_size(px(11.5))
                .text_color(p.muted)
                .line_height(relative(1.45))
                .child(t)
        };
        let mut body = div()
            .track_focus(&s.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                this.dictionary_key(ev, window, cx)
            }))
            .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.close_sheet(window, cx)
            }))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(13.))
                    .child("Your blyg's dictionary"),
            )
            .child(muted(
                "Words your blyg's spellcheck accepts, in Burrow, in Studio and in every other \
                 client. Learn Spelling in the editor adds a word here too. When a word \
                 Burrow's Learn Spelling taught this computer leaves your blyg's dictionary, \
                 here or in Studio, it's unlearned on this computer too; a word this computer \
                 learned any other way is never touched."
                    .into(),
            ));
        let d = match &s.load {
            DictLoad::Loading => {
                return (
                    460.,
                    body.child(muted("Reading it…".into())).into_any_element(),
                );
            }
            DictLoad::Unoffered => {
                return (
                    460.,
                    body.child(muted(
                        "Your blyg's studio doesn't keep a dictionary yet (studio 0.69). The \
                         spellcheck uses this computer's own list."
                            .into(),
                    ))
                    .into_any_element(),
                );
            }
            DictLoad::Failed(e) => {
                return (
                    460.,
                    body.child(
                        div()
                            .text_color(p.over_text())
                            .child(format!("Couldn't read it: {e}")),
                    )
                    .into_any_element(),
                );
            }
            DictLoad::Ready(d) => d,
        };
        let writable = d.writable;
        body = if writable {
            body.child(
                crate::theme_ext::field_box(div(), theme)
                    .px(px(10.))
                    .py(px(6.))
                    .text_size(px(13.))
                    .child(gpui_kit::base::input::Input::new(&s.add)),
            )
        } else {
            body.child(muted(READ_ONLY_NOTE.into()))
        };
        let words = d.owner_words();
        let mut list = div()
            .id("dictionary-words")
            .max_h(px(300.))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if words.is_empty() {
            list = list.child(muted("No words of your own yet.".into()));
        }
        for (i, w) in words.iter().enumerate() {
            let selected = i == s.sel;
            list = list.child(
                div()
                    .id(SharedString::from(format!("dictionary-word-{i}")))
                    .flex()
                    .items_center()
                    .px(px(8.))
                    .py(px(3.))
                    .rounded(px(5.))
                    .when(selected, |d| d.bg(p.sel))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .text_color(if selected { p.sel_ink } else { p.ink })
                            .child(w.clone()),
                    )
                    .when(writable, |row| {
                        row.child(
                            div()
                                .id(SharedString::from(format!("dictionary-remove-{i}")))
                                .px(px(6.))
                                .cursor_pointer()
                                .text_color(if selected { p.sel_muted } else { p.muted })
                                .child("×")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.dictionary_remove(i, window, cx)
                                })),
                        )
                    }),
            );
        }
        body = body
            .child(muted(format!(
                "{} word{} of yours",
                words.len(),
                if words.len() == 1 { "" } else { "s" }
            )))
            .child(list)
            .children(other_sources(d).map(muted));
        if let Some(e) = &s.error {
            body = body.child(div().text_color(p.over_text()).child(e.clone()));
        }
        if s.busy {
            body = body.child(muted("Saving…".into()));
        }
        let keys = if writable {
            crate::keymap::hint("⏎ adds · ↑↓ choose · ⌘⌫ takes out · esc closes").to_string()
        } else {
            "↑↓ choose · esc closes".to_string()
        };
        body = body.child(muted(keys));
        (460., body.into_any_element())
    }
}
