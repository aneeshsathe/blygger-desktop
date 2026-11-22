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
//! (`blyg_core::dictionary::Pending`) and sent by the next read.
//!
//! Keys: type a word and ⏎ adds it; ↑/↓ choose one of your words, ⌘⌫
//! takes it out; esc closes. A blyg without a dictionary (its capabilities
//! don't list `getDictionary`) says so.

use std::time::{Duration, Instant};

use blyg_core::dictionary::{Accepted, Dictionary, Edit, OWNER, word_problem};

use super::*;

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
        cx.set_global(LastRead(Some(Instant::now())));
        let backend = self.backend.clone();
        let learn = backend.clone();
        crate::composer::set_on_learn(
            std::sync::Arc::new(move |word: String| {
                let b = learn.clone();
                // A learned word goes on to the blyg off the UI thread.
                // Offline, the backend keeps it and the next read sends it;
                // a blyg without a dictionary leaves the system's alone.
                let _ = std::thread::Builder::new()
                    .name("blygger-dictionary-learn".into())
                    .spawn(move || {
                        if let Err(e) = b.edit_dictionary(&Edit::Add(word)) {
                            eprintln!("blygger: the blyg's dictionary didn't take the word: {e}");
                        }
                    });
            }),
            cx,
        );
        let task = cx.background_spawn(async move { backend.dictionary() });
        cx.spawn(async move |_, cx| {
            if let Ok(Some(d)) = task.await {
                cx.update(|cx| crate::composer::set_accepted(Accepted::from(&d), cx));
            }
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
        let task = cx.background_spawn(async move { backend.dictionary() });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, _, cx| v.dictionary_loaded(r.map(Some), cx));
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
        if s.busy || !matches!(s.load, DictLoad::Ready(_)) {
            return;
        }
        s.busy = true;
        s.error = None;
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.edit_dictionary(&edit) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, _, cx| match r {
                Ok(d) => v.dictionary_loaded(Ok(Some(Some(d))), cx),
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
                 client. Learn Spelling in the editor adds a word here too."
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
        body = body.child(
            crate::theme_ext::field_box(div(), theme)
                .px(px(10.))
                .py(px(6.))
                .text_size(px(13.))
                .child(gpui_kit::base::input::Input::new(&s.add)),
        );
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
                    .child(
                        div()
                            .id(SharedString::from(format!("dictionary-remove-{i}")))
                            .px(px(6.))
                            .cursor_pointer()
                            .text_color(if selected { p.sel_muted } else { p.muted })
                            .child("×")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.dictionary_remove(i, window, cx)
                            })),
                    ),
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
        body = body.child(muted(
            crate::keymap::hint("⏎ adds · ↑↓ choose · ⌘⌫ takes out · esc closes").to_string(),
        ));
        (460., body.into_any_element())
    }
}
