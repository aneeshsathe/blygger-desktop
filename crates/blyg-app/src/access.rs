//! --- access-ai --- Blyg › Client Access…: the apps and tokens that can act
//! as the owner on the blyg (studio's More › Client access, contract 5.7).
//!
//! - The list: every grant, with what it may do, when it was made and when
//!   it ends. Burrow's own browser sign-in is marked "this Burrow".
//! - `n` makes a manual API token: a name, REST API or MCP, and its
//!   permissions. The token is shown once, with Copy; Burrow never keeps it
//!   (it's dropped when the sheet closes, and never logged).
//! - `⌫` revokes the highlighted grant, `⇧⌫` every grant, each after a
//!   confirmation that warns when Burrow's own sign-in goes with it.
//!
//! The studio serves client access to its own sign-in only (a bearer token
//! gets 401), so a Burrow signed in with the browser or a token asks for the
//! studio password first; the session it makes lives in memory until Burrow
//! quits.
//!
//! A child module of `app`; `app.rs` keeps small hooks marked
//! `// --- access-ai ---`.

use blyg_core::access_ai::{
    AccessGate, Authorization, NewAuthorization, SCOPE_DESCRIPTIONS, TokenResource, is_own_grant,
};

use super::*;

gpui_kit::actions!(
    blygger,
    [
        /// Blyg › Client Access…
        ShowClientAccess
    ]
);

/// What the sheet shows.
pub enum Stage {
    /// The studio password, to open a session for client access.
    Password,
    Loading,
    List,
    /// "Revoke …?": one grant (its index) or all of them (`None`).
    Confirm(Option<usize>),
    /// The new-token form.
    NewToken {
        /// One per [`SCOPE_DESCRIPTIONS`] row.
        scope: [bool; 5],
        resource: TokenResource,
    },
    /// The token just made, shown once.
    Token {
        name: String,
        token: String,
    },
}

pub struct ClientAccessSheet {
    pub focus: FocusHandle,
    pub password: Entity<InputState>,
    pub name: Entity<InputState>,
    pub stage: Stage,
    pub rows: Vec<Authorization>,
    /// Burrow's own OAuth client id, when signed in with the browser.
    pub own: Option<String>,
    pub sel: usize,
    pub busy: bool,
    pub error: Option<String>,
}

impl ClientAccessSheet {
    pub fn inputs(&self) -> Vec<&Entity<InputState>> {
        vec![&self.password, &self.name]
    }
}

/// "REST API", "MCP", or both, from a grant's audience(s).
pub fn resource_label(resource: &str) -> &'static str {
    let mcp = resource.split_whitespace().any(|r| r.ends_with("/mcp"));
    let api = resource.split_whitespace().any(|r| !r.ends_with("/mcp"));
    match (api, mcp) {
        (true, true) => "REST API + MCP",
        (false, true) => "MCP",
        _ => "REST API",
    }
}

/// `owner:read owner:draft` → "read · draft".
pub fn scope_label(scope: &[String]) -> String {
    scope
        .iter()
        .map(|s| s.strip_prefix("owner:").unwrap_or(s))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A time against now, in days: "today", "3 days ago", "in 30 days".
pub fn days(at: i64, now: i64) -> String {
    let d = (at - now) / 86_400;
    match d {
        0 => "today".into(),
        1 => "tomorrow".into(),
        -1 => "yesterday".into(),
        d if d > 0 => format!("in {d} days"),
        d => format!("{} days ago", -d),
    }
}

/// What revoking says first.
pub fn confirm_text(
    rows: &[Authorization],
    which: Option<usize>,
    own: Option<&str>,
) -> (String, Option<&'static str>) {
    match which.and_then(|i| rows.get(i)) {
        Some(a) => (
            format!("Revoke “{}”?", a.name),
            Some(if is_own_grant(a, own) {
                "This is the Burrow you're using: it will be signed out of this blyg and need to sign in again."
            } else {
                "It can't act as you any more. It needs a new authorization to connect again; your studio sign-in stays."
            }),
        ),
        None => (
            "Revoke every client's access?".into(),
            Some(
                if rows.iter().any(|a| is_own_grant(a, own)) || own.is_none() {
                    "Every app and token signs out, this Burrow included if it signs in with the browser or a token. Studio sign-ins stay."
                } else {
                    "Every app and token signs out. Studio sign-ins stay."
                },
            ),
        ),
    }
}

impl MainView {
    /// Hook: the sheet's action on the main window.
    pub(super) fn access_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &ShowClientAccess, window, cx| {
                this.open_client_access(window, cx)
            }),
        )
    }

    /// Blyg › Client Access…: open it (or close it).
    pub(crate) fn open_client_access(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.sheet, Some(Sheet::ClientAccess(_))) {
            return self.close_sheet(window, cx);
        }
        let gate = self.backend.access_gate();
        if gate == AccessGate::Unavailable {
            return self.show_toast("Connect to a blyg to manage its client access", None, cx);
        }
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Studio password")
        });
        let name = cx.new(|cx| {
            InputState::new(window, cx).placeholder("A name for the token, e.g. “Feed checker”")
        });
        self._subs.push(
            cx.subscribe_in(&password, window, |this, _, ev, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.access_sign_in(window, cx);
                }
            }),
        );
        self._subs
            .push(cx.subscribe_in(&name, window, |this, _, ev, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.access_create(window, cx);
                }
            }));
        let focus = cx.focus_handle();
        let need = gate == AccessGate::NeedsPassword;
        if need {
            password.update(cx, |s, cx| s.focus(window, cx));
        } else {
            window.focus(&focus, cx);
        }
        self.sheet_gen += 1;
        self.sheet = Some(Sheet::ClientAccess(Box::new(ClientAccessSheet {
            focus,
            password,
            name,
            stage: if need {
                Stage::Password
            } else {
                Stage::Loading
            },
            rows: Vec::new(),
            own: self.backend.own_client_id(),
            sel: 0,
            busy: false,
            error: None,
        })));
        cx.notify();
        if !need {
            self.access_reload(window, cx);
        }
    }

    fn access_sheet(&mut self) -> Option<&mut ClientAccessSheet> {
        match self.sheet.as_mut() {
            Some(Sheet::ClientAccess(s)) => Some(s),
            _ => None,
        }
    }

    /// A failure on the sheet. A refused session goes back to the password.
    fn access_failed(&mut self, e: CoreError, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.access_sheet() else { return };
        s.busy = false;
        match e {
            CoreError::Unauthorized => {
                s.stage = Stage::Password;
                s.error = Some(
                    "The studio password was refused, or its sign-in ended. Type it again.".into(),
                );
                let pw = s.password.clone();
                pw.update(cx, |st, cx| {
                    st.set_value("", window, cx);
                    st.focus(window, cx)
                });
            }
            CoreError::Offline => {
                s.error = Some("Offline. Try again when you're back online.".into())
            }
            CoreError::NotFound => {
                s.error = Some(
                    "This blyg has no client access to manage (its studio is older than 0.28)."
                        .into(),
                )
            }
            e => s.error = Some(format!("{e}")),
        }
        cx.notify();
    }

    fn access_sign_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.access_sheet() else { return };
        if s.busy || !matches!(s.stage, Stage::Password) {
            return;
        }
        let pw = s.password.read(cx).value().to_string();
        if pw.is_empty() {
            return;
        }
        s.busy = true;
        s.error = None;
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.access_sign_in(&pw) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| match r {
                Ok(()) => {
                    if let Some(s) = v.access_sheet() {
                        s.busy = false;
                        s.stage = Stage::Loading;
                        let (pw, focus) = (s.password.clone(), s.focus.clone());
                        pw.update(cx, |st, cx| st.set_value("", window, cx));
                        window.focus(&focus, cx);
                    }
                    v.access_reload(window, cx);
                }
                Err(e) => v.access_failed(e, window, cx),
            });
        })
        .detach();
    }

    /// Read the grants again.
    fn access_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.authorizations() });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| match r {
                Ok(rows) => {
                    if let Some(s) = v.access_sheet() {
                        s.sel = s.sel.min(rows.len().saturating_sub(1));
                        s.rows = rows;
                        s.busy = false;
                        if matches!(s.stage, Stage::Loading | Stage::Confirm(_)) {
                            s.stage = Stage::List;
                        }
                        cx.notify();
                    }
                }
                Err(e) => v.access_failed(e, window, cx),
            });
        })
        .detach();
    }

    fn access_new_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.access_sheet() else { return };
        s.stage = Stage::NewToken {
            scope: [true, false, false, false, false],
            resource: TokenResource::Api,
        };
        s.error = None;
        let name = s.name.clone();
        name.update(cx, |st, cx| {
            st.set_value("", window, cx);
            st.focus(window, cx)
        });
        cx.notify();
    }

    pub(crate) fn access_toggle_scope(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(s) = self.access_sheet()
            && let Stage::NewToken { scope, .. } = &mut s.stage
            && let Some(on) = scope.get_mut(i)
        {
            *on = !*on;
            cx.notify();
        }
    }

    pub(crate) fn access_set_resource(&mut self, r: TokenResource, cx: &mut Context<Self>) {
        if let Some(s) = self.access_sheet()
            && let Stage::NewToken { resource, .. } = &mut s.stage
        {
            *resource = r;
            cx.notify();
        }
    }

    fn access_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.access_sheet() else { return };
        let Stage::NewToken { scope, resource } = &s.stage else {
            return;
        };
        if s.busy {
            return;
        }
        let name = s.name.read(cx).value().trim().to_string();
        let scope: Vec<String> = SCOPE_DESCRIPTIONS
            .iter()
            .zip(scope.iter())
            .filter(|(_, on)| **on)
            .map(|((k, _), _)| k.to_string())
            .collect();
        if name.is_empty() || name.chars().count() > 100 {
            s.error = Some("Give the token a name (up to 100 characters).".into());
            return cx.notify();
        }
        if scope.is_empty() {
            s.error = Some("Choose at least one permission.".into());
            return cx.notify();
        }
        let new = NewAuthorization {
            name,
            scope,
            resource: *resource,
        };
        s.busy = true;
        s.error = None;
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.create_authorization(&new) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| match r {
                Ok(t) => {
                    if let Some(s) = v.access_sheet() {
                        s.busy = false;
                        s.stage = Stage::Token {
                            name: t.authorization.name.clone(),
                            token: t.access_token,
                        };
                        s.rows.push(t.authorization);
                        s.sel = s.rows.len() - 1;
                        let focus = s.focus.clone();
                        window.focus(&focus, cx);
                        cx.notify();
                    }
                }
                Err(e) => v.access_failed(e, window, cx),
            });
        })
        .detach();
    }

    /// Copy the token just made; it stays on screen until the sheet moves on.
    pub(crate) fn access_copy_token(&mut self, cx: &mut Context<Self>) {
        let Some(Sheet::ClientAccess(s)) = &self.sheet else {
            return;
        };
        let Stage::Token { token, .. } = &s.stage else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(token.clone()));
        self.show_toast(
            "Token copied",
            Some("Paste it into your tool's credential store".into()),
            cx,
        );
    }

    /// Back to the list from the token (the token is dropped).
    fn access_done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.access_sheet() else { return };
        s.stage = Stage::List;
        s.error = None;
        let focus = s.focus.clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(crate) fn access_confirm(
        &mut self,
        which: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.access_sheet() else { return };
        if s.busy || (which.is_some() && s.rows.is_empty()) {
            return;
        }
        if which.is_none() && s.rows.is_empty() {
            return;
        }
        s.stage = Stage::Confirm(which);
        s.error = None;
        let focus = s.focus.clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(crate) fn access_revoke(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.access_sheet() else { return };
        let Stage::Confirm(which) = s.stage else {
            return;
        };
        if s.busy {
            return;
        }
        let target = which.and_then(|i| s.rows.get(i).cloned());
        if which.is_some() && target.is_none() {
            return;
        }
        let own = s.own.clone();
        let own_gone = match &target {
            Some(a) => is_own_grant(a, own.as_deref()),
            None => s.rows.iter().any(|a| is_own_grant(a, own.as_deref())),
        };
        s.busy = true;
        cx.notify();
        let backend = self.backend.clone();
        let id = target.as_ref().map(|a| a.id.clone());
        let task = cx.background_spawn(async move {
            match &id {
                Some(id) => backend.revoke_authorization(id),
                None => backend.revoke_all_authorizations(),
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| match r {
                Ok(()) => {
                    let head = match &target {
                        Some(a) => format!("Revoked “{}”", a.name),
                        None => "Revoked every client's access".to_string(),
                    };
                    let sub =
                        own_gone.then(|| "This Burrow signs in again: Blyg › Manage Blygs…".into());
                    v.show_toast(head, sub, cx);
                    v.access_reload(window, cx);
                }
                Err(e) => v.access_failed(e, window, cx),
            });
        })
        .detach();
    }

    fn access_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        #[derive(Clone, Copy, PartialEq)]
        enum At {
            Password,
            Loading,
            List,
            Confirm,
            New,
            Token,
        }
        let Some(s) = self.access_sheet() else { return };
        let at = match s.stage {
            Stage::Password => At::Password,
            Stage::Loading => At::Loading,
            Stage::List => At::List,
            Stage::Confirm(_) => At::Confirm,
            Stage::NewToken { .. } => At::New,
            Stage::Token { .. } => At::Token,
        };
        let (n, sel) = (s.rows.len(), s.sel);
        let key = ev.keystroke.key.as_str();
        let shift = ev.keystroke.modifiers.shift;
        match (at, key) {
            (At::Confirm | At::New | At::Token, "escape") => self.access_done(window, cx),
            (_, "escape") => self.close_sheet(window, cx),
            (At::List, "up") => {
                if let Some(s) = self.access_sheet() {
                    s.sel = sel.saturating_sub(1);
                }
                cx.notify();
            }
            (At::List, "down") => {
                if let Some(s) = self.access_sheet() {
                    s.sel = (sel + 1).min(n.saturating_sub(1));
                }
                cx.notify();
            }
            (At::List, "backspace" | "delete") if shift => self.access_confirm(None, window, cx),
            (At::List, "backspace" | "delete") => self.access_confirm(Some(sel), window, cx),
            (At::List, "n") => self.access_new_token(window, cx),
            (At::List, "r") => self.access_reload(window, cx),
            (At::Confirm, "enter") => self.access_revoke(window, cx),
            (At::Token, "c") => self.access_copy_token(cx),
            (At::Token, "enter") => self.access_done(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    /// `BLYGGER_DEMO=aa-…` (screenshots, fake mode): client access (its
    /// list, a revoke of Burrow's own grant, a new token), AI on the blyg,
    /// and the publish sheet with the blyg's AI.
    pub(crate) fn access_ai_demo(
        &mut self,
        scenario: &str,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sign_in = |v: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
            v.open_client_access(window, cx);
            if let Some(Sheet::ClientAccess(s)) = &v.sheet {
                let pw = s.password.clone();
                pw.update(cx, |e, cx| e.set_value("studio password", window, cx));
            }
            v.access_sign_in(window, cx);
        };
        match (scenario, n) {
            ("aa-access" | "aa-confirm" | "aa-token", 0) => sign_in(self, window, cx),
            ("aa-confirm", 1) => self.access_confirm(Some(0), window, cx),
            ("aa-token", 1) => {
                self.access_new_token(window, cx);
                if let Some(Sheet::ClientAccess(s)) = &self.sheet {
                    let name = s.name.clone();
                    name.update(cx, |e, cx| e.set_value("Deploy script", window, cx));
                }
                self.access_toggle_scope(1, cx);
            }
            ("aa-token", 2) => self.access_create(window, cx),
            ("aa-ai", 0) => self.open_server_ai(window, cx),
            ("aa-publish", 0) => {
                self.open(&LocalId("01J9PX1".into()), window, cx);
                self.demo_type(" Then the rain came.", window, cx);
            }
            ("aa-publish", 1) => self.publish(window, cx),
            ("aa-publish", 2) => self.draft_change_note(window, cx),
            _ => {}
        }
    }

    pub(super) fn render_access_sheet(
        &self,
        s: &ClientAccessSheet,
        cx: &mut Context<Self>,
    ) -> (f32, AnyElement) {
        let p = self.palette;
        let theme = &*self.theme;
        let kbd = |k: &'static str| {
            crate::theme_ext::kbd(div(), theme)
                .px(px(5.))
                .py(px(1.))
                .min_w(px(18.))
                .flex()
                .justify_center()
                .text_size(px(10.5))
                .child(crate::keymap::hint(k))
        };
        let hint = |k: &'static str, label: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .child(kbd(k))
                .child(label)
        };
        let keys = |items: Vec<Div>| {
            div()
                .mt(px(12.))
                .flex()
                .flex_wrap()
                .gap(px(14.))
                .text_size(px(11.5))
                .text_color(p.muted)
                .children(items)
        };
        let field = |el: AnyElement| {
            crate::theme_ext::field_box(div(), theme)
                .px(px(10.))
                .py(px(7.))
                .text_size(px(13.5))
                .child(el)
        };
        let muted = |t: String| {
            div()
                .text_color(p.muted)
                .line_height(relative(1.45))
                .child(t)
        };
        let host = self
            .backend
            .base_url()
            .map(|u| super::reading::vm::host(&u))
            .unwrap_or_else(|| "your blyg".into());
        let mut body = div()
            .track_focus(&s.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| this.access_key(ev, window, cx)))
            .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                let back = matches!(
                    &this.sheet,
                    Some(Sheet::ClientAccess(s)) if matches!(s.stage, Stage::NewToken { .. })
                );
                if back {
                    this.access_done(window, cx)
                } else {
                    this.close_sheet(window, cx)
                }
            }))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .mb(px(4.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).text_size(px(13.)).child("Client access"))
                    .child(div().text_size(px(11.)).text_color(p.muted).child(host.clone())),
            )
            .child(muted(format!(
                "Apps and tokens that can act as you on {host}, without your password. Revoke any of them here."
            )));
        let now = chrono::Utc::now().timestamp();
        match &s.stage {
            Stage::Password => {
                body = body
                    .child(div().mt(px(10.)).child(muted(
                        "Your blyg shows client access to its own sign-in only. Type the studio password: Burrow uses it once and keeps the sign-in in memory until it quits."
                            .into(),
                    )))
                    .child(div().mt(px(8.)).child(field(
                        gpui_kit::base::input::Input::new(&s.password).into_any_element(),
                    )))
                    .child(keys(vec![
                        hint("⏎", if s.busy { "signing in…" } else { "sign in" }),
                        hint("esc", "close"),
                    ]));
            }
            Stage::Loading => {
                body = body.child(div().mt(px(10.)).text_color(p.muted).child("Loading…"));
            }
            Stage::List | Stage::Confirm(_) => {
                let confirm = match s.stage {
                    Stage::Confirm(w) => Some(w),
                    _ => None,
                };
                let mut list = div()
                    .id("access-list")
                    .mt(px(10.))
                    .max_h(px(360.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(2.));
                if s.rows.is_empty() {
                    list = list.child(muted(
                        "No app or token can act as you. Press n to make a token.".into(),
                    ));
                }
                for (i, a) in s.rows.iter().enumerate() {
                    let selected = i == s.sel;
                    let own = is_own_grant(a, s.own.as_deref());
                    let badge = |t: &'static str, accent: bool| {
                        div()
                            .px(px(5.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(if accent { p.accent } else { p.line })
                            .text_size(px(10.5))
                            .text_color(if accent {
                                p.accent_text()
                            } else if selected {
                                p.sel_muted
                            } else {
                                p.muted
                            })
                            .child(t)
                    };
                    let when = match a.expires_at {
                        Some(e) => {
                            format!("made {} · ends {}", days(a.created_at, now), days(e, now))
                        }
                        None => format!("made {}", days(a.created_at, now)),
                    };
                    list = list.child(
                        div()
                            .id(SharedString::from(format!("access-row-{i}")))
                            .debug_selector(move || format!("access-row-{i}"))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .px(px(8.))
                            .py(px(6.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .when(selected, |d| d.bg(p.sel))
                            .hover(|d| d.bg(p.sel))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(1.))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.))
                                            .child(
                                                div()
                                                    .truncate()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(if selected {
                                                        p.sel_ink
                                                    } else {
                                                        p.ink
                                                    })
                                                    .child(a.name.clone()),
                                            )
                                            .when(own, |d| d.child(badge("this Burrow", true)))
                                            .child(badge(
                                                if a.manual { "token" } else { "sign-in" },
                                                false,
                                            ))
                                            .child(badge(resource_label(&a.resource), false)),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(11.))
                                            .text_color(if selected {
                                                p.sel_muted
                                            } else {
                                                p.muted
                                            })
                                            .child(format!("{} · {when}", scope_label(&a.scope))),
                                    ),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("access-revoke-{i}")))
                                    .text_size(px(11.))
                                    .text_color(if selected { p.sel_muted } else { p.muted })
                                    .hover(|d| d.text_color(p.over))
                                    .child("Revoke…")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if let Some(Sheet::ClientAccess(s)) = this.sheet.as_mut() {
                                            s.sel = i;
                                        }
                                        this.access_confirm(Some(i), window, cx)
                                    })),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(Sheet::ClientAccess(s)) = this.sheet.as_mut() {
                                    s.sel = i;
                                    cx.notify();
                                }
                            })),
                    );
                }
                body = body.child(list);
                if let Some(which) = confirm {
                    let (q, warn) = confirm_text(&s.rows, which, s.own.as_deref());
                    let own_hit = match which.and_then(|i| s.rows.get(i)) {
                        Some(a) => is_own_grant(a, s.own.as_deref()),
                        None => true,
                    };
                    body = body.child(
                        div()
                            .mt(px(10.))
                            .p(px(10.))
                            .rounded(px(self.theme.corner(7.)))
                            .border_1()
                            .border_color(if own_hit { p.over } else { p.line })
                            .child(div().font_weight(FontWeight::MEDIUM).child(q))
                            .when_some(warn, |d, w| {
                                d.child(
                                    div()
                                        .mt(px(4.))
                                        .line_height(relative(1.45))
                                        .text_color(if own_hit { p.warn_text() } else { p.muted })
                                        .child(w),
                                )
                            })
                            .child(keys(vec![
                                hint("⏎", if s.busy { "revoking…" } else { "revoke" }),
                                hint("esc", "keep"),
                            ])),
                    );
                } else {
                    body = body.child(
                        keys(vec![
                            hint("↑↓", "choose"),
                            hint("⌫", "revoke"),
                            hint("⇧⌫", "revoke all"),
                            hint("n", "new token"),
                            hint("r", "reload"),
                            hint("esc", "close"),
                        ])
                        .child(
                            crate::theme_ext::chip(div(), theme, false)
                                .id("access-new")
                                .px(px(8.))
                                .cursor_pointer()
                                .child("New token…")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.access_new_token(window, cx)
                                })),
                        ),
                    );
                }
            }
            Stage::NewToken { scope, resource } => {
                let check =
                    |i: usize, on: bool, label: &'static str, what: &'static str| {
                        div()
                            .id(SharedString::from(format!("access-scope-{i}")))
                            .flex()
                            .gap(px(8.))
                            .py(px(2.))
                            .cursor_pointer()
                            .child(if on { "☑" } else { "☐" })
                            .child(
                                div().flex().flex_col().child(label).child(
                                    div().text_size(px(11.)).text_color(p.muted).child(what),
                                ),
                            )
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.access_toggle_scope(i, cx)),
                            )
                    };
                let radio = |r: TokenResource, label: &'static str| {
                    let on = *resource == r;
                    div()
                        .id(SharedString::from(format!("access-res-{}", r.as_str())))
                        .flex()
                        .gap(px(6.))
                        .cursor_pointer()
                        .child(if on { "◉" } else { "○" })
                        .child(label)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.access_set_resource(r, cx)),
                        )
                };
                let label = |t: &'static str| {
                    div()
                        .mt(px(10.))
                        .mb(px(4.))
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.muted)
                        .child(t)
                };
                body = body
                    .child(label("NEW TOKEN · for tools that can't sign in with the browser; it lasts 30 days"))
                    .child(field(gpui_kit::base::input::Input::new(&s.name).into_any_element()))
                    .child(label("FOR"))
                    .child(
                        div()
                            .flex()
                            .gap(px(16.))
                            .child(radio(TokenResource::Api, "the REST API"))
                            .child(radio(TokenResource::Mcp, "MCP")),
                    )
                    .child(label("IT MAY"))
                    .children(
                        SCOPE_DESCRIPTIONS
                            .iter()
                            .enumerate()
                            .map(|(i, (k, what))| check(i, scope[i], k, what)),
                    )
                    .child(keys(vec![
                        hint("⏎", if s.busy { "making it…" } else { "make the token" }),
                        hint("esc", "back"),
                    ]));
            }
            Stage::Token { name, token } => {
                body = body
                    .child(
                        div()
                            .mt(px(10.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!("“{name}”: copy your token now")),
                    )
                    .child(muted(
                        "It appears this once. Burrow doesn't keep it: store it in your tool's credential store."
                            .into(),
                    ))
                    .child(
                        crate::theme_ext::field_box(div(), theme)
                            .id("access-token")
                            .mt(px(8.))
                            .px(px(10.))
                            .py(px(8.))
                            .font_family(if cfg!(target_os = "windows") { "Consolas" } else { "Menlo" })
                            .text_size(px(12.))
                            .child(token.clone()),
                    )
                    .child(
                        keys(vec![hint("c", "copy"), hint("⏎", "done")]).child(
                            crate::theme_ext::chip(div(), theme, true)
                                .id("access-copy")
                                .px(px(8.))
                                .cursor_pointer()
                                .child("Copy")
                                .on_click(cx.listener(|this, _, _, cx| this.access_copy_token(cx))),
                        ),
                    );
            }
        }
        if let Some(e) = &s.error {
            body = body.child(div().mt(px(8.)).text_color(p.over_text()).child(e.clone()));
        }
        (520., body.into_any_element())
    }
}
// --- end access-ai ---

#[cfg(test)]
mod tests {
    use super::{Authorization, confirm_text, days, resource_label, scope_label};

    fn grant(name: &str, client: &str) -> Authorization {
        Authorization {
            id: format!("g-{name}"),
            client_id: client.into(),
            name: name.into(),
            manual: false,
            resource: "https://blyg.example.com/api".into(),
            scope: vec!["owner:read".into(), "reading:state".into()],
            created_at: 0,
            expires_at: None,
        }
    }

    #[test]
    fn labels_read_plainly() {
        assert_eq!(resource_label("https://blyg.example.com/api"), "REST API");
        assert_eq!(resource_label("https://blyg.example.com/mcp"), "MCP");
        assert_eq!(
            resource_label("https://blyg.example.com/api https://blyg.example.com/mcp"),
            "REST API + MCP"
        );
        assert_eq!(scope_label(&grant("a", "c").scope), "read · reading:state");
        assert_eq!(days(86_400 * 3, 0), "in 3 days");
        assert_eq!(days(0, 86_400 * 12), "12 days ago");
        assert_eq!(days(10, 0), "today");
    }

    #[test]
    fn revoking_burrows_own_grant_warns() {
        let rows = vec![grant("Burrow", "me"), grant("Other", "them")];
        let (q, w) = confirm_text(&rows, Some(0), Some("me"));
        assert_eq!(q, "Revoke “Burrow”?");
        assert!(w.unwrap().contains("the Burrow you're using"));
        let (_, w) = confirm_text(&rows, Some(1), Some("me"));
        assert!(!w.unwrap().contains("Burrow you're using"));
        let (q, w) = confirm_text(&rows, None, Some("me"));
        assert!(q.contains("every client"));
        assert!(w.unwrap().contains("this Burrow included"));
    }
}
