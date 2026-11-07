//! --- contacts --- The Contacts sheet: the book (search, rows with Edit,
//! Check page and Remove, "+ Add contact"), or the add / edit form.

use gpui_kit::base::input::{Escape, Input};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{ContactsSheet, Form, NOT_SERVED};
use crate::app::{MainView, Sheet};

/// What the sheet says about handles (Studio's address-book hint).
pub(crate) const ABOUT: &str = "Type @handle while writing. When you publish, it becomes a link \
to the person, and the handle stays here, private to you. A mention notifies nobody, and \
past versions keep the link they were published with.";

impl MainView {
    pub(crate) fn render_contacts_sheet(
        &self,
        s: &ContactsSheet,
        cx: &mut Context<Self>,
    ) -> (f32, AnyElement) {
        let p = self.palette.on_page();
        let heading = match (&s.offer, &s.form) {
            (Some(o), _) => o.heading.clone(),
            (None, Some(f)) if f.editing.is_some() => "Edit contact".into(),
            (None, Some(_)) => "Add a contact".into(),
            (None, None) => "Contacts".into(),
        };
        let mut body =
            div()
                .id("contacts-sheet")
                .track_focus(&s.focus)
                .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                    cx.stop_propagation();
                    this.cancel_contact_form(window, cx);
                }))
                .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    this.contacts_key(ev, window, cx)
                }))
                .flex()
                .flex_col()
                .child(self.sheet_heading(heading));
        if let Some(q) = s.offer.as_ref().and_then(|o| o.question.clone()) {
            body = body.child(div().mb(px(4.)).font_weight(FontWeight::MEDIUM).child(q));
        }
        if s.served == Some(false) {
            return (
                480.,
                body.child(div().text_color(p.muted).child(NOT_SERVED))
                    .child(self.keys_row(vec![self.key_hint("esc", "close")]))
                    .into_any_element(),
            );
        }
        if let Some(f) = &s.form {
            return (
                520.,
                body.child(self.render_contact_form(f, s, cx))
                    .into_any_element(),
            );
        }
        body = body
            .child(
                div()
                    .mb(px(8.))
                    .text_color(p.muted)
                    .line_height(relative(1.4))
                    .child(ABOUT),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .child(self.input_box(Input::new(&s.search).into_any_element(), false)),
                    )
                    .child(
                        self.chip("contact-add", "+ Add contact")
                            .debug_selector(|| "contact-add".into())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.new_contact(window, cx)),
                            ),
                    ),
            );
        if let Some(e) = &s.error {
            body = body.child(div().mt(px(6.)).text_color(p.over).child(e.clone()));
        }
        let shown = s.shown(cx);
        let rows =
            div()
                .id("contacts-list")
                .mt(px(8.))
                .max_h(px(360.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .when(shown.is_empty(), |d| {
                    d.child(div().py(px(10.)).text_color(p.muted).child(
                    if s.book.is_empty() {
                        if s.served.is_none() {
                            "Loading your contacts…"
                        } else {
                            "No contacts yet. + Add contact, or Post › Add Author to Contacts… \
                             on a post you're reading."
                        }
                    } else {
                        "No contact matches."
                    },
                ))
                })
                .children(shown.into_iter().enumerate().map(|(i, c)| {
                    let selected = i == s.sel;
                    let (edit, check, remove) = (c.id.clone(), c.id.clone(), c.id.clone());
                    let confirming = s.confirm_remove.as_deref() == Some(c.id.as_str());
                    let checking = s.checking.as_deref() == Some(c.id.as_str());
                    let checked = c.checked_ids().len();
                    let open = c.origin.clone();
                    div()
                        .id(SharedString::from(format!("contact-{i}")))
                        .debug_selector(move || format!("contact-row-{i}"))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(8.))
                        .py(px(6.))
                        .rounded(px(6.))
                        .when(selected, |d| d.bg(p.sel))
                        .hover(|d| d.bg(p.sel))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(6.))
                                        .items_baseline()
                                        .child(
                                            div()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(p.accent)
                                                .child(format!("@{}", c.handle)),
                                        )
                                        .child(
                                            div()
                                                .truncate()
                                                .font_weight(FontWeight::MEDIUM)
                                                .child(c.display_name().to_string()),
                                        ),
                                )
                                .child(
                                    div()
                                        .id(SharedString::from(format!("contact-url-{i}")))
                                        .truncate()
                                        .text_size(px(11.5))
                                        .text_color(p.muted)
                                        .cursor_pointer()
                                        .hover(|d| d.text_color(p.accent))
                                        .on_click(move |_, _, cx| cx.open_url(&open))
                                        .child(match checked {
                                            0 => c.origin.clone(),
                                            1 => format!("{} · 1 checked address", c.origin),
                                            n => format!("{} · {n} checked addresses", c.origin),
                                        }),
                                ),
                        )
                        .child(self.chip(format!("contact-edit-{i}"), "Edit").on_click(
                            cx.listener(move |this, _, window, cx| {
                                this.edit_contact(edit.clone(), window, cx)
                            }),
                        ))
                        .child(
                            self.chip(
                                format!("contact-check-{i}"),
                                if checking {
                                    "Checking…"
                                } else {
                                    "Check page"
                                },
                            )
                            .tooltip(|_, cx| {
                                cx.new(|_| {
                                    crate::app::reading::Tip(
                                        "Your blyg reads their page for the other addresses it \
                                     lists as theirs (rel=\"me\"); those go in the link"
                                            .into(),
                                    )
                                })
                                .into()
                            })
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.check_contact(check.clone(), cx),
                            )),
                        )
                        .child(
                            self.chip(
                                format!("contact-remove-{i}"),
                                if confirming {
                                    "Remove? click again"
                                } else {
                                    "Remove"
                                },
                            )
                            .when(confirming, |d| d.border_color(p.over).text_color(p.over))
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.remove_contact(remove.clone(), cx),
                            )),
                        )
                }));
        body = body.child(rows).child(self.keys_row(vec![
            self.key_hint("↑↓", "move"),
            self.key_hint("⏎", "edit"),
            self.key_hint("esc", "close"),
        ]));
        (580., body.into_any_element())
    }

    fn render_contact_form(&self, f: &Form, s: &ContactsSheet, cx: &mut Context<Self>) -> Div {
        let p = self.palette.on_page();
        let label = |t: &'static str, help: &'static str| {
            div()
                .mt(px(10.))
                .mb(px(4.))
                .flex()
                .gap(px(8.))
                .items_baseline()
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.muted)
                        .child(t),
                )
                .child(div().text_size(px(11.)).text_color(p.muted).child(help))
        };
        let field = |e: &gpui_kit::Entity<gpui_kit::base::input::InputState>| {
            self.input_box(Input::new(e).into_any_element(), false)
        };
        let save = if s.offer.is_some() || f.editing.is_none() {
            "add contact"
        } else {
            "save"
        };
        let cancel = if s.offer.is_some() { "not now" } else { "back" };
        div()
            .child(label("HANDLE", "private to you"))
            .child(field(&f.handle))
            .child(label("DISPLAY NAME", "shown on the public page"))
            .child(field(&f.name))
            .child(label(
                "URL",
                "where the person can be found: a blyg, or a social profile",
            ))
            .child(field(&f.url))
            .when_some(f.error.clone(), |d, e| {
                d.child(div().mt(px(6.)).text_color(p.over).child(e))
            })
            .when(f.busy, |d| {
                d.child(div().mt(px(6.)).text_color(p.muted).child("Saving…"))
            })
            .child(
                self.keys_row(vec![])
                    .child(crate::platform::key_button(
                        self.key_hint("⏎", save),
                        "contact-save",
                        cx.listener(|this, _, window, cx| this.save_contact(window, cx)),
                    ))
                    .child(self.key_hint("tab", "next field"))
                    .child(crate::platform::key_button(
                        self.key_hint("esc", cancel),
                        "contact-cancel",
                        cx.listener(|this, _, window, cx| this.cancel_contact_form(window, cx)),
                    )),
            )
    }

    /// The sheet's keys: tab between the form's fields; ↑/↓ over the list.
    fn contacts_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::Contacts(s)) = self.sheet.as_mut() else {
            return;
        };
        let key = ev.keystroke.key.as_str();
        if s.form.is_some() {
            if key == "tab" {
                cx.stop_propagation();
                let back = ev.keystroke.modifiers.shift;
                self.contact_form_tab(back, window, cx);
            }
            return;
        }
        let n = s.shown(cx).len();
        match key {
            "down" => s.sel = (s.sel + 1).min(n.saturating_sub(1)),
            "up" => s.sel = s.sel.saturating_sub(1),
            _ => return,
        }
        s.confirm_remove = None;
        cx.stop_propagation();
        cx.notify();
    }
}
