//! Connect (first run, or signed out: an address and a way in) and
//! Disconnect (forget this blyg; `1` keeps the local copy, `2` deletes it).
//! The sign-in itself is `MainView::submit_connect` and its helpers.

use std::sync::Arc;

use gpui_kit::base::input::{Escape, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Kit, SheetView, Shown, Submit};
use crate::app::{CONNECT_INTRO, MainView};

/// How the Connect sheet signs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectWay {
    /// OAuth in the system browser (studio 0.28+; the default).
    Browser,
    /// A Studio API token, or a server's owner token.
    Token,
    /// The studio password (any studio; the fallback).
    Password,
}

/// Sets its flag when dropped: cancels a waiting browser sign-in.
pub(crate) struct CancelOnDrop(pub(crate) Arc<std::sync::atomic::AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// First run (no `blyg-url`): URL + a way in (browser sign-in, API token or
/// studio password), verified with `GET /api/items`, then → Keychain →
/// config.
pub(crate) struct ConnectSheet {
    pub(crate) url: Entity<InputState>,
    /// The studio password, or the API token (`way`).
    pub(crate) token: Entity<InputState>,
    pub(crate) way: ConnectWay,
    pub(crate) error: Option<String>,
    /// The check (or the browser sign-in) is running.
    pub(crate) busy: bool,
    /// A browser sign-in waiting for the owner: dropping it (closing or
    /// replacing the sheet, choosing another way) cancels the wait.
    pub(crate) waiting: Option<CancelOnDrop>,
}

/// Forget this blyg: `1` keeps the local copy, `2` deletes it too.
pub(crate) struct DisconnectSheet {
    pub(crate) host: String,
    pub(crate) focus: FocusHandle,
}

impl SheetView for ConnectSheet {
    fn render(&self, _: &MainView, kit: &Kit, cx: &mut Context<MainView>) -> Shown {
        let p = kit.p;
        let ConnectSheet {
            url,
            token,
            way,
            error,
            busy,
            waiting,
        } = self;
        Shown::from({
            let way = *way;
            let waiting = waiting.is_some();
            // "Sign in with browser · API token · Studio password": the
            // chosen one in ink.
            let choice = |id: &'static str, text: &'static str, pick: ConnectWay| {
                let on = way == pick;
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .cursor_pointer()
                    .text_color(if on { p.ink } else { p.muted })
                    .when(on, |d| d.font_weight(FontWeight::SEMIBOLD))
                    .hover(|s| s.underline())
                    .child(text)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_connect_way(pick, window, cx)
                    }))
            };
            let label = |s: &'static str| {
                div()
                    .mt(px(10.))
                    .mb(px(4.))
                    .text_size(px(10.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.muted)
                    .child(s)
            };
            let note = |s: &'static str| {
                div()
                    .mt(px(8.))
                    .text_size(px(12.))
                    .text_color(p.muted)
                    .line_height(relative(1.45))
                    .child(s)
            };
            let explain = match way {
                ConnectWay::Browser => {
                    "Burrow opens your blyg's studio in your browser. Sign in there and \
                         allow Burrow; it gets renewable access with every permission, \
                         kept in your macOS Keychain."
                }
                ConnectWay::Token => {
                    "Make a token in your blyg's Studio → More → Client access: choose REST \
                         API and every permission (read, draft, publish, manage, and reading:state \
                         where offered). It lasts \
                         30 days. It is kept in your macOS Keychain."
                }
                ConnectWay::Password => {
                    "The password you sign in to the studio with. It is kept in your \
                         macOS Keychain."
                }
            };
            (
                480.,
                div()
                    .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                        cx.stop_propagation();
                        this.close_sheet(window, cx);
                    }))
                    .child(kit.heading("Connect your blyg".into()))
                    .child(
                        div()
                            .text_color(p.muted)
                            .line_height(relative(1.45))
                            .child(CONNECT_INTRO),
                    )
                    .child(label("BLYG ADDRESS"))
                    .child(kit.input_box(gpui_kit::base::input::Input::new(url).into_any_element()))
                    .child(
                        div()
                            .mt(px(10.))
                            .flex()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .child(choice(
                                "connect-by-browser",
                                "Sign in with browser",
                                ConnectWay::Browser,
                            ))
                            .child(div().text_color(p.muted).child("·"))
                            .child(choice("connect-by-token", "API token", ConnectWay::Token))
                            .child(div().text_color(p.muted).child("·"))
                            .child(choice(
                                "connect-by-password",
                                "Studio password",
                                ConnectWay::Password,
                            )),
                    )
                    .child(note(crate::keymap::hint(explain)))
                    .when(way != ConnectWay::Browser, |d| {
                        d.child(label(if way == ConnectWay::Password {
                            "STUDIO PASSWORD"
                        } else {
                            "API TOKEN"
                        }))
                        .child(
                            kit.input_box(
                                gpui_kit::base::input::Input::new(token).into_any_element(),
                            ),
                        )
                    })
                    .when_some(error.clone(), |d, e| {
                        d.child(div().mt(px(8.)).text_color(p.over_text()).child(e))
                    })
                    .when(*busy, |d| {
                        d.child(div().mt(px(8.)).text_color(p.muted).child(match way {
                            ConnectWay::Browser if waiting => {
                                "Waiting for you to allow Burrow in your browser… (esc cancels)"
                            }
                            ConnectWay::Browser => "Checking the address…",
                            ConnectWay::Password => "Signing in…",
                            ConnectWay::Token => "Checking the address and token…",
                        }))
                    })
                    .child({
                        let next = if way == ConnectWay::Browser {
                            "next · open the browser"
                        } else {
                            "next · connect"
                        };
                        // The keys are buttons too on Windows (`platform::key_button`).
                        kit.keys_row(vec![])
                            .child(crate::platform::key_button(
                                kit.key_hint("⏎", next),
                                "connect-submit",
                                cx.listener(|this, _, window, cx| this.submit_connect(window, cx)),
                            ))
                            .child(crate::platform::key_button(
                                kit.key_hint("esc", "not now"),
                                "connect-cancel",
                                cx.listener(|this, _, window, cx| this.close_sheet(window, cx)),
                            ))
                    })
                    .into_any_element(),
            )
        })
    }

    fn inputs(&self) -> Vec<&Entity<InputState>> {
        vec![&self.url, &self.token]
    }

    fn submit(&self) -> Option<Submit> {
        Some(|v, window, cx| v.submit_connect(window, cx))
    }
}

impl SheetView for DisconnectSheet {
    fn render(&self, _: &MainView, kit: &Kit, cx: &mut Context<MainView>) -> Shown {
        let p = kit.p;
        let DisconnectSheet { host, focus } = self;
        Shown::from((
            460.,
            div()
                .track_focus(focus)
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    match ev.keystroke.key.as_str() {
                        "1" => this.disconnect_now(false, window, cx),
                        "2" => this.disconnect_now(true, window, cx),
                        "escape" => this.close_sheet(window, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                }))
                .child(kit.heading(format!("Disconnect from {host}?")))
                .child(div().text_color(p.muted).line_height(relative(1.45)).child(
                    crate::keymap::hint(
                        "The password or owner token is removed from your Keychain and the \
                             address from your config file. Nothing changes on the blyg itself.",
                    ),
                ))
                // The keys are buttons too on Windows (`platform::key_button`).
                .child(
                    kit.keys_row(vec![])
                        .child(crate::platform::key_button(
                            kit.key_hint(
                                "1",
                                crate::keymap::hint("disconnect, keep the posts on this Mac"),
                            ),
                            "disconnect-keep",
                            cx.listener(|this, _, window, cx| {
                                this.disconnect_now(false, window, cx)
                            }),
                        ))
                        .child(crate::platform::key_button(
                            kit.key_hint("2", "also delete the local copy"),
                            "disconnect-delete",
                            cx.listener(|this, _, window, cx| {
                                this.disconnect_now(true, window, cx)
                            }),
                        )),
                )
                .child(kit.keys_row(vec![]).child(crate::platform::key_button(
                    kit.key_hint("esc", "cancel"),
                    "disconnect-cancel",
                    cx.listener(|this, _, window, cx| this.close_sheet(window, cx)),
                )))
                .into_any_element(),
        ))
    }
}
