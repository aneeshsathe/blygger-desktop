//! Connecting a blyg and disconnecting it: the flow behind the Connect and
//! Disconnect sheets (the sheets themselves are in `sheet/connect.rs`).

use super::*;

impl MainView {
    /// No blyg connected, no posts and nothing typed: the empty list offers
    /// Connect (and sample data), and Enter connects.
    pub(crate) fn offers_connect(&self, cx: &App) -> bool {
        crate::connection::mode(cx) == Some(crate::connection::Mode::Disconnected)
            && self.list.results().is_empty()
            && self.list.trimmed_query().is_empty()
    }

    pub(super) fn open_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://blyg.example.com"));
        let token = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("API token")
        });
        let t2 = token.clone();
        self._subs.push(
            cx.subscribe_in(&url, window, move |this, _, ev, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    // Signing in with the browser needs only the address.
                    if this
                        .sheet
                        .get::<ConnectSheet>()
                        .is_some_and(|s| s.way == ConnectWay::Browser)
                    {
                        this.submit_connect(window, cx);
                    } else {
                        t2.update(cx, |s, cx| s.focus(window, cx));
                    }
                }
            }),
        );
        self._subs
            .push(cx.subscribe_in(&token, window, |this, _, ev, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.submit_connect(window, cx);
                }
            }));
        // A configured blyg without a usable credential (the Keychain entry
        // went away, or its browser sign-in ended): offer its address.
        let mut error = None;
        if let Some(u) = crate::settings::blyg_url(cx) {
            let tokens = crate::settings::get(cx).tokens.clone();
            if let Ok(Some(g)) = blyg_core::config::load_grant(tokens.as_ref(), &u)
                && g.ended()
            {
                error = Some(format!(
                    "Your browser sign-in to {} has ended. Sign in again.",
                    vm::url_host(&u).unwrap_or(u.clone())
                ));
            }
            url.update(cx, |s, cx| s.set_value(u, window, cx));
        }
        url.update(cx, |s, cx| s.focus(window, cx));
        self.sheet.open(ConnectSheet {
            url,
            token,
            way: ConnectWay::Browser,
            error,
            busy: false,
            waiting: None,
        });
        cx.notify();
    }

    /// The Connect sheet's way in: browser sign-in, API token or password.
    pub(crate) fn set_connect_way(
        &mut self,
        pick: ConnectWay,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ConnectSheet {
            url,
            token,
            way,
            error,
            busy,
            waiting,
        }) = self.sheet.get_mut()
        {
            *way = pick;
            *error = None;
            // Choosing another way cancels a browser sign-in in progress.
            if waiting.take().is_some() {
                *busy = false;
            }
            match pick {
                ConnectWay::Browser => url.update(cx, |s, cx| s.focus(window, cx)),
                ConnectWay::Token | ConnectWay::Password => {
                    let hint = if pick == ConnectWay::Password {
                        "Studio password"
                    } else {
                        "API token"
                    };
                    token.update(cx, |s, cx| {
                        s.set_value("", window, cx);
                        s.set_placeholder(hint, window, cx);
                        s.focus(window, cx);
                    });
                }
            }
        }
        cx.notify();
    }

    pub(super) fn submit_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ConnectSheet {
            url,
            token,
            way,
            busy,
            ..
        }) = self.sheet.get()
        else {
            return;
        };
        if *busy {
            return;
        }
        let way = *way;
        let (u, t) = (
            url.read(cx).value().to_string(),
            token.read(cx).value().to_string(),
        );
        if way == ConnectWay::Browser {
            let checked = crate::settings::validate_blyg_url(&u)
                .and_then(|u| crate::settings::check_one_per_host(&u, cx).map(|()| u));
            match checked {
                Ok(u) => self.start_browser_sign_in(u, window, cx),
                Err(e) => self.set_connect_state(Some(e), false, cx),
            }
            return;
        }
        let by_password = way == ConnectWay::Password;
        let checked = crate::settings::validate_blyg_url(&u).and_then(|u| {
            // A password is taken as typed; a pasted token is trimmed.
            let secret = if by_password {
                t.clone()
            } else {
                t.trim().to_string()
            };
            match (secret.is_empty(), by_password) {
                (true, true) => Err("Type your blyg's studio password".to_string()),
                (true, false) => {
                    Err("Paste an API token: make one in Studio → More → Client access".to_string())
                }
                (false, true) => Ok((u, Credential::Password(secret))),
                (false, false) => Ok((u, Credential::Token(secret))),
            }
        });
        let (u, t) = match checked {
            Ok(v) => v,
            Err(e) => {
                self.set_connect_state(Some(e), false, cx);
                return;
            }
        };
        // Verify before saving anything: sign in, then GET /api/items.
        self.set_connect_state(None, true, cx);
        let verify = crate::connection::verifier(cx);
        let task = cx.background_spawn({
            let (u, t) = (u.clone(), t.clone());
            async move { verify(&u, &t) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let verdict = task.await;
            let _ = this.update_in(cx, |v, window, cx| match verdict {
                Err(e) => v.set_connect_state(Some(e.to_string()), false, cx),
                Ok(_) => v.finish_connect(&u, &t, window, cx),
            });
        })
        .detach();
    }

    /// Sign in with the browser: discover and register (in the
    /// background), open the authorize page, wait for the loopback
    /// callback, exchange the code, verify, then save like any credential.
    pub(super) fn start_browser_sign_in(
        &mut self,
        url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only against a real blyg: never from tests or BLYGGER_FAKE's
        // sample data (registering a client writes to the server).
        let real = cx
            .try_global::<crate::connection::Connection>()
            .is_some_and(|c| {
                c.switch.mode() != crate::connection::Mode::Fake
                    || cx.has_global::<crate::connection::SampleSession>()
            });
        if !real {
            return self.set_connect_state(
                Some("Browser sign-in needs a real blyg; use a token or password here.".into()),
                false,
                cx,
            );
        }
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if let Some(ConnectSheet {
            error,
            busy,
            waiting,
            ..
        }) = self.sheet.get_mut()
        {
            *error = None;
            *busy = true;
            *waiting = Some(CancelOnDrop(cancel.clone()));
        }
        cx.notify();
        let tokens = crate::settings::get(cx).tokens.clone();
        let verify = crate::connection::verifier(cx);
        let start = cx.background_spawn({
            let url = url.clone();
            async move { blyg_core::api::oauth::BrowserSignIn::start(&url, tokens.as_ref()) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let started = start.await;
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return; // the sheet closed, or another way was chosen
            }
            let sign_in = match started {
                Ok(Some(s)) => s,
                other => {
                    let _ = this.update_in(cx, |v, window, cx| {
                        let msg = match other {
                            Ok(_) => {
                                // No OAuth routes: the password works.
                                v.set_connect_way(ConnectWay::Password, window, cx);
                                "This blyg has no browser sign-in. Use its studio password."
                                    .to_string()
                            }
                            Err(CoreError::Offline) => {
                                "Couldn't reach that address. Check the URL and your connection."
                                    .into()
                            }
                            Err(e) => e.to_string(),
                        };
                        v.set_connect_state(Some(msg), false, cx);
                        v.clear_connect_wait();
                    });
                    return;
                }
            };
            let _ = this.update(cx, |_, cx| cx.open_url(&sign_in.authorize_url));
            let task = cx.background_spawn({
                let (url, cancel) = (url.clone(), cancel.clone());
                async move {
                    let grant = sign_in.finish(&cancel, blyg_core::api::oauth::SIGN_IN_TIMEOUT)?;
                    let cred = Credential::OAuth(blyg_core::api::oauth::OAuthSession::new(grant));
                    verify(&url, &cred)
                        .map(|_| cred)
                        .map_err(|e| CoreError::Other(e.to_string()))
                }
            });
            let verdict = task.await;
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return; // the sheet closed, or another way was chosen
            }
            let _ = this.update_in(cx, |v, window, cx| {
                v.clear_connect_wait();
                match verdict {
                    Ok(cred) => v.finish_connect(&url, &cred, window, cx),
                    Err(CoreError::Offline) => v.set_connect_state(
                        Some(
                            "Couldn't reach that address. Check the URL and your connection."
                                .into(),
                        ),
                        false,
                        cx,
                    ),
                    Err(e) => v.set_connect_state(Some(e.to_string()), false, cx),
                }
            });
        })
        .detach();
    }

    /// The browser sign-in is over: drop its cancel flag (nothing waits on
    /// it any more).
    pub(super) fn clear_connect_wait(&mut self) {
        if let Some(ConnectSheet { waiting, .. }) = self.sheet.get_mut() {
            waiting.take();
        }
    }

    pub(super) fn set_connect_state(
        &mut self,
        err: Option<String>,
        is_busy: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(ConnectSheet { error, busy, .. }) = self.sheet.get_mut() {
            *error = err;
            *busy = is_busy;
        }
        cx.notify();
    }

    /// The check passed: save the credential and URL, switch to the live
    /// backend, and load everything once.
    pub(super) fn finish_connect(
        &mut self,
        url: &str,
        cred: &Credential,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let url = match crate::settings::connect(url, cred, cx) {
            Ok(u) => u,
            Err(e) => return self.set_connect_state(Some(e), false, cx),
        };
        let host = vm::url_host(&url).unwrap_or(url.clone());
        match crate::connection::go_live(&url, cx) {
            Ok(true) => {
                self.sheet.close();
                self.base_url = self.backend.base_url();
                self.refresh_problems(cx);
                self.focus_after_sheet(window, cx);
                self.initial_sync(Some(host), window, cx);
            }
            Ok(false) => {
                // Fake mode (or headless tests): nothing to switch.
                self.sheet.close();
                self.refresh_problems(cx);
                self.focus_after_sheet(window, cx);
                if self.backend.server_outdated() {
                    return self.server_outdated(cx);
                }
                self.show_toast(
                    format!("Connected to {host}"),
                    Some(
                        crate::keymap::hint(match cred {
                            Credential::Password(_) => "The password is in your Keychain",
                            Credential::Token(_) => "The token is in your Keychain",
                            Credential::OAuth(_) => "Your sign-in is in your Keychain",
                        })
                        .into(),
                    ),
                    cx,
                );
            }
            Err(e) => self.set_connect_state(
                Some(format!("Couldn't open the local database: {e}")),
                false,
                cx,
            ),
        }
        cx.notify();
    }

    pub(super) fn on_disconnect(
        &mut self,
        _: &Disconnect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(url) = crate::settings::blyg_url(cx) else {
            return self.show_toast("Not connected to a blyg", None, cx);
        };
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.sheet.open(DisconnectSheet {
            host: vm::url_host(&url).unwrap_or(url),
            focus,
        });
        cx.notify();
    }

    /// Forget the token and `blyg-url`; optionally delete the local copy.
    pub(super) fn disconnect_now(
        &mut self,
        delete_local: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sheet.close();
        match crate::connection::disconnect(delete_local, cx) {
            Ok(host) => {
                self.current = None;
                self.base_url = None;
                self.loading = false;
                self.unread = 0;
                self.refresh_problems(cx);
                self.requery(window, cx);
                self.show_toast(
                    format!("Disconnected from {host}"),
                    Some(if delete_local {
                        "The token and the local copy are gone".into()
                    } else {
                        crate::keymap::hint("The token is gone; your posts stay on this Mac").into()
                    }),
                    cx,
                );
                // --- blyg switcher --- the next signed-in blyg is active now;
                // with none left (or none that opens), connect one.
                if crate::settings::blyg_url(cx).is_some()
                    && crate::connection::mode(cx) != Some(crate::connection::Mode::Disconnected)
                {
                    self.blyg_switched(window, cx);
                } else {
                    self.open_connect(window, cx);
                }
            }
            Err(e) => self.show_toast(e, None, cx),
        }
        cx.notify();
    }
}
