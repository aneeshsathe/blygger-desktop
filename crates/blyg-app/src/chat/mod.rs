//! --- chat --- The experimental #thicket tab: IRC on Libera.Chat, one
//! channel, beside the views in the title bar. Experimental: it may be
//! removed.
//!
//! - The tab (`render_chat_tab`, in the view switcher) shows "#thicket",
//!   an "experimental" badge, a dot and count for unread messages, and an
//!   accent "@n" when someone mentions your nick. ⇧⌘I (Blyg › #thicket
//!   Chat) brings it to the front, and again goes back to Posts.
//! - Setup (`Setup`), until `chat-nick` is set: sign in with a Libera.Chat
//!   account (SASL PLAIN over TLS; nothing is saved unless it works), or
//!   create one: a nick, a password and an email, `NickServ REGISTER`, then
//!   the emailed code (`NickServ VERIFY REGISTER`). **Nothing connects
//!   until the user acts in setup.** After setup the password is in the
//!   Keychain (`irc:libera:<nick>`, the app's own service), the nick in the
//!   config, and Burrow signs in on its own at each launch.
//! - The screen: the log (timestamps, nicks, /me, joins and parts kept
//!   quiet), a user list (ops first, marked), and a compose box (⏎ sends,
//!   `/me` acts). The connection's state sits quietly in the header.
//! - A mention while the tab isn't in front posts a system notification
//!   (`notify.rs`; `chat-notifications`, Settings).
//!
//! Protocol and connection live in `blyg-irc` (a thread per client, no
//! async runtime). A child module of `app` (it reads `MainView`); `app.rs`
//! and `reading/mod.rs` keep small hooks marked `// --- chat ---`.

pub mod notify;
pub mod vm;

#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex};

use blyg_core::config::Change;
use blyg_irc::{
    Auth, Client, Command, Endpoint, Event, LineKind, MsgKind, Room, SessionConfig, THICKET, Timing,
};

use super::*;
use crate::app::reading::View;

pub use vm::{Conn, EXPERIMENTAL};

gpui_kit::actions!(blygger, [ShowChat]);

/// Lines kept in memory (nothing is written to disk).
pub const LOG_CAP: usize = 1000;
/// Notifications remembered for the tests (and nothing else).
const NOTIFIED_CAP: usize = 20;

/// What the running client is for.
#[derive(Clone)]
pub(crate) enum Role {
    /// Signed in for good: chat.
    Live,
    /// Setup's "I have an account": save the password once SASL works.
    SignIn { nick: String, password: String },
    /// Setup's "Create an account": REGISTER once welcomed, then VERIFY.
    Register {
        nick: String,
        password: String,
        email: String,
    },
}

/// Where setup is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// "I have an account" or "Create one".
    Choose,
    SignIn,
    Register,
    /// REGISTER sent: waiting for the emailed code.
    Verify,
}

pub struct Setup {
    pub step: Step,
    pub nick: Entity<InputState>,
    pub password: Entity<InputState>,
    pub email: Entity<InputState>,
    pub code: Entity<InputState>,
    pub error: Option<String>,
    /// Working (connecting, signing in, waiting for NickServ).
    pub busy: Option<&'static str>,
    /// What NickServ said, verbatim.
    pub notices: Vec<String>,
}

pub struct Chat {
    pub room: Room,
    pub conn: Conn,
    client: Option<Client>,
    role: Role,
    /// Bumped per client: events from an older one are dropped.
    generation: u64,
    inbox: Arc<Mutex<Vec<(u64, Event)>>>,
    wake: Option<async_channel::Sender<()>>,
    pub unread: usize,
    pub mentions: usize,
    pub setup: Option<Setup>,
    pub compose: Option<Entity<InputState>>,
    pub focus: Option<FocusHandle>,
    scroll: ScrollHandle,
    /// Tests point the client at the in-process server.
    pub endpoint: Option<Endpoint>,
    pub timing: Timing,
    /// Notifications posted (title, body), newest last; for the tests.
    pub notified: Vec<(String, String)>,
}

impl Default for Chat {
    fn default() -> Self {
        Chat {
            room: Room::new(THICKET, "", LOG_CAP),
            conn: Conn::Off,
            client: None,
            role: Role::Live,
            generation: 0,
            inbox: Default::default(),
            wake: None,
            unread: 0,
            mentions: 0,
            setup: None,
            compose: None,
            focus: None,
            scroll: ScrollHandle::new(),
            endpoint: None,
            timing: Timing::default(),
            notified: Vec::new(),
        }
    }
}

impl Chat {
    fn stop_client(&mut self) {
        self.generation += 1;
        if let Some(c) = self.client.take() {
            // Off the UI thread: stopping waits for the goodbye.
            std::thread::spawn(move || c.stop());
        }
    }

    pub fn running(&self) -> bool {
        self.client.is_some()
    }
}

/// The config's chat keys, as the tab needs them.
struct ChatConfig {
    tab: bool,
    nick: Option<String>,
    server: String,
    notifications: bool,
}

fn config(cx: &App) -> ChatConfig {
    match cx.try_global::<crate::settings::AppConfig>() {
        Some(a) => {
            let c = a.store.config();
            ChatConfig {
                tab: c.chat_tab(),
                nick: c.chat_nick().map(str::to_string),
                server: c.chat_server().to_string(),
                notifications: c.chat_notifications(),
            }
        }
        None => ChatConfig {
            tab: true,
            nick: None,
            server: blyg_irc::LIBERA.into(),
            notifications: true,
        },
    }
}

impl MainView {
    /// Hook: the chat's actions on the main window.
    pub(super) fn chat_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &ShowChat, window, cx| this.show_chat(window, cx)))
    }

    /// Hook (new): the event pump, then sign in if set up.
    pub(super) fn chat_init(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.chat.focus = Some(cx.focus_handle());
        // GPUI's test scheduler refuses wake-ups from a foreign thread (the
        // client's), so the tests hand events over with `chat_drain`.
        if !cfg!(test) {
            self.chat_pump(window, cx);
        }
        self.chat_autoconnect(cx);
    }

    fn chat_pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (tx, rx) = async_channel::unbounded::<()>();
        self.chat.wake = Some(tx);
        cx.spawn_in(window, async move |this, cx| {
            while rx.recv().await.is_ok() {
                if this
                    .update_in(cx, |v, window, cx| v.chat_drain(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// Whether the tab is on (`chat-tab`).
    pub(crate) fn chat_tab_on(&self, cx: &App) -> bool {
        config(cx).tab
    }

    /// The server to use: the tests' override, else `chat-server` (TLS).
    /// Tests never reach anything but 127.0.0.1.
    fn chat_endpoint(&self, cx: &App) -> Option<Endpoint> {
        let ep = match &self.chat.endpoint {
            Some(e) => e.clone(),
            None => Endpoint::parse(&config(cx).server)?,
        };
        if cfg!(test) && !ep.is_loopback() {
            return None;
        }
        Some(ep)
    }

    /// `host:port` of the server in use (the tests' override, or
    /// `chat-server`): the Keychain account is named after it.
    fn chat_server(&self, cx: &App) -> String {
        match &self.chat.endpoint {
            Some(e) => format!("{}:{}", e.host, e.port),
            None => config(cx).server,
        }
    }

    fn chat_password(&self, nick: &str, cx: &App) -> Option<String> {
        let tokens = cx
            .try_global::<crate::settings::AppConfig>()?
            .tokens
            .clone();
        let account = vm::keychain_account(&self.chat_server(cx), nick);
        tokens.get(&account).ok().flatten()
    }

    /// Sign in on its own when set up: the tab on, a nick in the config
    /// and its password in the Keychain. Never on the sample data.
    pub(crate) fn chat_autoconnect(&mut self, cx: &mut Context<Self>) {
        let c = config(cx);
        if !c.tab
            || self.chat.running()
            || crate::connection::mode(cx) == Some(crate::connection::Mode::Fake)
        {
            return;
        }
        let Some(nick) = c.nick else { return };
        let Some(password) = self.chat_password(&nick, cx) else {
            return;
        };
        self.chat.room.me = nick.clone();
        self.chat_start(
            Role::Live,
            SessionConfig::new(
                &nick,
                Auth::Sasl {
                    account: nick.clone(),
                    password,
                },
                &[THICKET],
            ),
            cx,
        );
    }

    fn chat_start(&mut self, role: Role, session: SessionConfig, cx: &mut Context<Self>) {
        let Some(ep) = self.chat_endpoint(cx) else {
            self.chat.conn = Conn::Failed("The chat-server setting isn't host:port.".into());
            return;
        };
        self.chat.stop_client();
        let generation = self.chat.generation;
        let inbox = self.chat.inbox.clone();
        let wake = self.chat.wake.clone();
        self.chat.role = role;
        self.chat.conn = Conn::Connecting;
        self.chat.client = Some(Client::start(
            ep,
            session,
            self.chat.timing.clone(),
            move |e| {
                if let Ok(mut q) = inbox.lock() {
                    q.push((generation, e));
                }
                if let Some(w) = &wake {
                    let _ = w.try_send(());
                }
            },
        ));
        cx.notify();
    }

    /// Hand the client's events to the screen (also called by the tests).
    pub(crate) fn chat_drain(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let events: Vec<(u64, Event)> = match self.chat.inbox.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => return,
        };
        let generation = self.chat.generation;
        for (g, e) in events {
            if g == generation {
                self.chat_event(e, window, cx);
            }
        }
    }

    /// One event from the client.
    pub(crate) fn chat_event(&mut self, ev: Event, window: &mut Window, cx: &mut Context<Self>) {
        let now = vm::now();
        match &ev {
            Event::Connecting { .. } => self.chat.conn = Conn::Connecting,
            Event::Connected => self.chat.conn = Conn::SigningIn,
            Event::Registered { .. } => {
                self.chat.conn = Conn::Connected;
                if let Role::Register {
                    password, email, ..
                } = &self.chat.role
                    && let Some(c) = &self.chat.client
                {
                    c.send(Command::Register {
                        password: password.clone(),
                        email: email.clone(),
                    });
                    if let Some(s) = self.chat.setup.as_mut() {
                        s.step = Step::Verify;
                        s.busy = None;
                        s.error = None;
                    }
                    self.chat_focus_setup(window, cx);
                }
            }
            Event::LoggedIn { .. } => {
                if let Role::SignIn { nick, password } = self.chat.role.clone() {
                    self.chat_finish(&nick, &password, window, cx);
                }
            }
            Event::SaslFailed { reason } => {
                if let Some(s) = self.chat.setup.as_mut() {
                    s.error = Some(reason.clone());
                    s.busy = None;
                }
            }
            Event::Disconnected { reason, retry_in } => {
                self.chat.room.clear_members();
                match retry_in {
                    Some(d) if matches!(self.chat.role, Role::Live) => {
                        if self.chat.conn == Conn::Connected {
                            self.chat.room.status("Disconnected. Reconnecting…", now);
                        }
                        self.chat.conn = Conn::Retrying(*d);
                    }
                    _ => {
                        // Setup's connections don't retry on their own.
                        let setup = !matches!(self.chat.role, Role::Live);
                        self.chat.stop_client();
                        if setup {
                            self.chat.conn = Conn::Off;
                            if let Some(s) = self.chat.setup.as_mut() {
                                s.busy = None;
                                if s.error.is_none() && s.step != Step::Verify {
                                    s.error = Some(format!("Couldn't connect: {reason}"));
                                }
                            }
                        } else {
                            self.chat.conn = Conn::Failed(format!(
                                "Signing in failed: {reason} Set up #thicket again to fix it."
                            ));
                        }
                    }
                }
            }
            Event::Message(m)
                if m.kind == MsgKind::Notice && m.from.eq_ignore_ascii_case("NickServ") =>
            {
                if let Some(s) = self.chat.setup.as_mut() {
                    s.busy = None;
                    s.notices.push(m.text.clone());
                    if s.notices.len() > 8 {
                        s.notices.remove(0);
                    }
                }
                if vm::says_verified(&m.text)
                    && let Role::Register { nick, password, .. } = self.chat.role.clone()
                {
                    self.chat_finish(&nick, &password, window, cx);
                    // Again, signed in with SASL this time, into the channel.
                    self.chat.stop_client();
                    self.chat_autoconnect(cx);
                }
            }
            _ => {}
        }
        let applied = self.chat.room.apply(&ev, now);
        if applied.line {
            self.chat.scroll.scroll_to_bottom();
        }
        // The tab's marks count what came in while it wasn't showing; a
        // mention also notifies while Burrow isn't the active app.
        let on_tab = self.reading.view == View::Chat;
        let active = window.is_window_active();
        if applied.chat && !on_tab {
            self.chat.unread += 1;
        }
        if applied.mention && !on_tab {
            self.chat.mentions += 1;
        }
        if applied.mention
            && (!on_tab || !active)
            && config(cx).notifications
            && let Some(line) = self.chat.room.log().last()
        {
            let title = if line.private {
                format!("{} (private)", line.nick)
            } else {
                THICKET.to_string()
            };
            let body = vm::notification_body(&line.nick, &line.text);
            if !cfg!(test) {
                notify::post(&title, &body, !active);
            }
            self.chat.notified.push((title, body));
            if self.chat.notified.len() > NOTIFIED_CAP {
                self.chat.notified.remove(0);
            }
        }
        cx.notify();
    }

    /// Setup worked: the password to the Keychain, the nick to the config,
    /// and from now on Burrow signs in on its own.
    fn chat_finish(
        &mut self,
        nick: &str,
        password: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let account = vm::keychain_account(&self.chat_server(cx), nick);
        let saved = cx
            .try_global::<crate::settings::AppConfig>()
            .map(|a| a.tokens.clone())
            .map(|t| t.set(&account, password));
        if let Some(Err(e)) = saved {
            if let Some(s) = self.chat.setup.as_mut() {
                s.error = Some(format!("Couldn't keep the password in the Keychain: {e}"));
            }
            return;
        }
        if let Err(e) = crate::settings::write(&[("chat-nick", Change::Set(nick.to_string()))], cx)
        {
            self.show_toast(e, None, cx);
        }
        // Our own write isn't an edit to reload.
        self.watch_stamp = crate::settings::watch_stamp(&crate::settings::watched(cx));
        self.chat.role = Role::Live;
        self.chat.setup = None;
        self.chat.room.me = nick.to_string();
        self.chat.room.status(
            &format!("Signed in as {nick}. Welcome to #thicket."),
            vm::now(),
        );
        // The setup fields are gone: the keyboard goes to the compose box.
        self.chat_ensure_inputs(window, cx);
        if self.reading.view == View::Chat
            && let Some(c) = &self.chat.compose
        {
            c.update(cx, |s, cx| s.focus(window, cx));
        }
        cx.notify();
    }

    /// ⇧⌘I: bring the tab to the front (again: back to Posts).
    pub(crate) fn show_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chat_tab_on(cx) {
            self.show_toast(
                "The #thicket chat tab is off",
                Some(crate::keymap::hint("Settings (⌘,) › Chat turns it on").into()),
                cx,
            );
            return;
        }
        self.show_view(View::Chat, window, cx);
    }

    /// Hook (show_view): the tab came to the front.
    pub(crate) fn chat_opened(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.chat.unread = 0;
        self.chat.mentions = 0;
        self.chat_ensure_inputs(window, cx);
        if self.chat_needs_setup(cx) {
            self.chat_focus_setup(window, cx);
        } else if let Some(c) = &self.chat.compose {
            c.update(cx, |s, cx| s.focus(window, cx));
        }
        self.chat.scroll.scroll_to_bottom();
        cx.notify();
    }

    /// Setup shows until there's a nick, or while it's under way (and
    /// always in the tour, which never connects).
    pub(crate) fn chat_needs_setup(&self, cx: &App) -> bool {
        self.onboarding.tutorial.is_some() || self.chat.setup.is_some() || config(cx).nick.is_none()
    }

    fn chat_ensure_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.chat.compose.is_none() {
            let compose = cx.new(|cx| {
                InputState::new(window, cx).placeholder("Message #thicket  (/me to act)")
            });
            self._subs.push(
                cx.subscribe_in(&compose, window, |this, _, ev, window, cx| {
                    if let InputEvent::PressEnter { .. } = ev {
                        this.chat_send(window, cx);
                    }
                }),
            );
            self.chat.compose = Some(compose);
        }
        if self.chat.setup.is_none() && self.chat_needs_setup(cx) {
            self.chat.setup = Some(self.chat_new_setup(window, cx));
        }
    }

    fn chat_new_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Setup {
        let input =
            |ph: &'static str, masked: bool, window: &mut Window, cx: &mut Context<Self>| {
                cx.new(|cx| InputState::new(window, cx).masked(masked).placeholder(ph))
            };
        let nick = input("ada_example", false, window, cx);
        let password = input("At least 8 characters, no spaces", true, window, cx);
        let email = input("you@example.com", false, window, cx);
        let code = input("The code from the email", false, window, cx);
        for (field, next) in [
            (nick.clone(), Some(password.clone())),
            (password.clone(), Some(email.clone())),
            (email.clone(), None),
            (code.clone(), None),
        ] {
            let is_password = field == password;
            self._subs.push(
                cx.subscribe_in(&field, window, move |this, _, ev, window, cx| {
                    if !matches!(ev, InputEvent::PressEnter { .. }) {
                        return;
                    }
                    let step = this.chat.setup.as_ref().map(|s| s.step);
                    match (step, &next) {
                        // Signing in needs no email: ⏎ in the password goes.
                        (Some(Step::SignIn), _) if is_password => this.chat_submit(window, cx),
                        (_, Some(n)) => n.update(cx, |s, cx| s.focus(window, cx)),
                        _ => this.chat_submit(window, cx),
                    }
                }),
            );
        }
        if let Some(n) = config(cx).nick {
            nick.update(cx, |s, cx| s.set_value(n, window, cx));
        }
        Setup {
            step: Step::Choose,
            nick,
            password,
            email,
            code,
            error: None,
            busy: None,
            notices: Vec::new(),
        }
    }

    fn chat_focus_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = &self.chat.setup else { return };
        let field = match s.step {
            Step::Choose => None,
            Step::SignIn | Step::Register => Some(s.nick.clone()),
            Step::Verify => Some(s.code.clone()),
        };
        match field {
            Some(f) => f.update(cx, |s, cx| s.focus(window, cx)),
            None => {
                if let Some(f) = &self.chat.focus {
                    window.focus(f, cx);
                }
            }
        }
    }

    /// Setup: pick "sign in" or "create an account" (1 / 2, or a click).
    pub(crate) fn chat_choose(&mut self, step: Step, window: &mut Window, cx: &mut Context<Self>) {
        self.chat_ensure_inputs(window, cx);
        if let Some(s) = self.chat.setup.as_mut() {
            s.step = step;
            s.error = None;
        }
        self.chat_focus_setup(window, cx);
        cx.notify();
    }

    /// Setup's ⏎: sign in, register, or verify.
    pub(crate) fn chat_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.chat.setup.as_ref() else {
            return;
        };
        if self.onboarding.tutorial.is_some() {
            self.show_toast(
                "The tour doesn't connect",
                Some("Set up #thicket after the tour".into()),
                cx,
            );
            return;
        }
        let value = |e: &Entity<InputState>, cx: &App| e.read(cx).value().to_string();
        let step = s.step;
        let nick = vm::check_nick(&value(&s.nick, cx));
        let password = value(&s.password, cx);
        let email = value(&s.email, cx);
        let code = value(&s.code, cx);
        let result: Result<(), String> = (|| match step {
            Step::Choose => Ok(()),
            Step::SignIn => {
                let nick = nick?;
                if password.is_empty() {
                    return Err("Enter your password.".into());
                }
                self.chat.room = Room::new(THICKET, &nick, LOG_CAP);
                self.chat_start(
                    Role::SignIn {
                        nick: nick.clone(),
                        password: password.clone(),
                    },
                    SessionConfig::new(
                        &nick,
                        Auth::Sasl {
                            account: nick.clone(),
                            password: password.clone(),
                        },
                        &[THICKET],
                    ),
                    cx,
                );
                Ok(())
            }
            Step::Register => {
                let nick = nick?;
                vm::check_password(&password)?;
                let email = vm::check_email(&email)?;
                self.chat.room = Room::new(THICKET, &nick, LOG_CAP);
                let mut session = SessionConfig::new(&nick, Auth::None, &[]);
                session.alt_nicks = false;
                self.chat_start(
                    Role::Register {
                        nick,
                        password: password.clone(),
                        email,
                    },
                    session,
                    cx,
                );
                Ok(())
            }
            Step::Verify => {
                let Role::Register { nick, .. } = &self.chat.role else {
                    return Err("Start again: the connection that registered has ended.".into());
                };
                let Some(code) = vm::verify_code(&code) else {
                    return Err("Paste the code from Libera.Chat's email.".into());
                };
                match &self.chat.client {
                    Some(c) => {
                        c.send(Command::Verify {
                            account: nick.clone(),
                            code,
                        });
                        Ok(())
                    }
                    None => Err("Start again: the connection that registered has ended.".into()),
                }
            }
        })();
        if let Some(s) = self.chat.setup.as_mut() {
            match result {
                Ok(()) => {
                    s.error = None;
                    s.busy = match step {
                        Step::SignIn => Some("Signing in to Libera.Chat…"),
                        Step::Register => Some("Connecting to Libera.Chat…"),
                        Step::Verify => Some("Asking NickServ…"),
                        Step::Choose => None,
                    };
                }
                Err(e) => s.error = Some(e),
            }
        }
        let _ = window;
        cx.notify();
    }

    /// Setup's "Back": to the choice, dropping a connection setup made.
    pub(crate) fn chat_setup_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.chat.role, Role::Live) {
            self.chat.stop_client();
            self.chat.role = Role::Live;
            self.chat.conn = Conn::Off;
        }
        self.chat_choose(Step::Choose, window, cx);
    }

    /// ⏎ in the compose box.
    pub(crate) fn chat_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(compose) = self.chat.compose.clone() else {
            return;
        };
        let text = compose.read(cx).value().to_string();
        let cmd = match vm::compose(&text) {
            vm::Compose::Nothing => return,
            vm::Compose::Say(t) => Command::Say {
                target: THICKET.into(),
                text: t,
            },
            vm::Compose::Me(t) => Command::Me {
                target: THICKET.into(),
                text: t,
            },
            vm::Compose::Unknown(c) => {
                self.show_toast(
                    format!("{c} isn't something this tab does"),
                    Some("Only /me, for now".into()),
                    cx,
                );
                return;
            }
        };
        match (&self.chat.client, &self.chat.conn) {
            (Some(c), Conn::Connected) if self.chat.room.joined => {
                c.send(cmd);
                compose.update(cx, |s, cx| s.set_value("", window, cx));
            }
            _ => self.show_toast(
                "Not connected yet",
                Some("Your message is still in the box".into()),
                cx,
            ),
        }
    }

    /// "Sign out": forget the password and nick; back to setup.
    pub(crate) fn chat_sign_out(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let c = config(cx);
        let server = self.chat_server(cx);
        self.chat.stop_client();
        self.chat.conn = Conn::Off;
        if let Some(nick) = &c.nick
            && let Some(a) = cx.try_global::<crate::settings::AppConfig>()
        {
            let _ = a.tokens.delete(&vm::keychain_account(&server, nick));
        }
        if let Err(e) = crate::settings::write(&[("chat-nick", Change::Remove)], cx) {
            self.show_toast(e, None, cx);
        }
        self.watch_stamp = crate::settings::watch_stamp(&crate::settings::watched(cx));
        self.chat.room = Room::new(THICKET, "", LOG_CAP);
        self.chat.unread = 0;
        self.chat.mentions = 0;
        self.chat.setup = None;
        self.chat_ensure_inputs(window, cx);
        self.chat_focus_setup(window, cx);
        cx.notify();
    }

    /// Hook (config reload): the tab turned off disconnects; on, it signs in.
    pub(super) fn chat_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let c = config(cx);
        if !c.tab {
            self.chat.stop_client();
            self.chat.conn = Conn::Off;
            if self.reading.view == View::Chat {
                self.show_view(View::Posts, window, cx);
            }
        } else if matches!(self.chat.role, Role::Live) {
            self.chat_autoconnect(cx);
        }
    }

    /// Settings' switches: `chat-tab` and `chat-notifications`.
    pub(crate) fn set_chat_setting(
        &mut self,
        key: &'static str,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(e) = crate::settings::write(&[(key, Change::Set(on.to_string()))], cx) {
            self.show_toast(e, None, cx);
            return;
        }
        self.watch_stamp = crate::settings::watch_stamp(&crate::settings::watched(cx));
        if key == "chat-tab" {
            self.chat_reload(window, cx);
        }
        cx.notify();
    }

    /// Hook (tour): leave the chat screen.
    pub(crate) fn chat_tour_tidy(&mut self) {
        if self.reading.view == View::Chat {
            self.reading.view = View::Posts;
        }
    }

    // ------------------------------------------------------------ render

    /// Hook (view switcher): the tab, after the views (`None` when off).
    pub(crate) fn render_chat_tab(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.chat_tab_on(cx) {
            return None;
        }
        let p = self.palette.on_bar();
        let on = self.reading.view == View::Chat;
        let mark = vm::mark(self.chat.unread, self.chat.mentions);
        let tip = match (&mark, &self.chat.conn) {
            (vm::Mark::Mention(n), _) => format!(
                "#thicket · {} mentioned you · {}",
                if *n == 1 {
                    "someone".into()
                } else {
                    format!("{n} messages")
                },
                crate::keymap::hint("⇧⌘I")
            ),
            (_, c) => format!(
                "#thicket on Libera.Chat · {} · {} · {EXPERIMENTAL}",
                vm::conn_text(c),
                crate::keymap::hint("⇧⌘I")
            ),
        };
        let (badge_fg, badge_bg) = self.palette.quiet_badge();
        Some(
            div()
                .id("view-chat")
                .debug_selector(|| "view-chat".into())
                .px(px(9.))
                .py(px(2.))
                .rounded_full()
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(4.))
                .when(on, |d| d.bg(p.sel).text_color(p.ink))
                .when(!on, |d| d.hover(|s| s.text_color(p.ink)))
                .child(THICKET)
                .child(
                    div()
                        .px(px(4.))
                        .rounded(px(3.))
                        .text_size(px(8.5))
                        .text_color(badge_fg)
                        .bg(badge_bg)
                        .child("experimental"),
                )
                .map(|d| match mark {
                    vm::Mark::None => d,
                    vm::Mark::Unread(n) => d
                        .child(div().size(px(5.)).rounded_full().bg(p.accent))
                        .child(div().text_size(px(10.)).child(vm::count(n))),
                    vm::Mark::Mention(n) => d.child(
                        div()
                            .px(px(4.))
                            .rounded_full()
                            .bg(p.accent)
                            .text_color(self.palette.bg)
                            .text_size(px(10.))
                            .font_weight(FontWeight::BOLD)
                            .child(format!("@{}", vm::count(n))),
                    ),
                })
                .tooltip(move |_, cx| cx.new(|_| reading::Tip(tip.clone())).into())
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.reading.view != View::Chat {
                        this.show_chat(window, cx)
                    }
                }))
                .into_any_element(),
        )
    }

    /// Hook (Settings): the CHAT row, `chat-tab` and `chat-notifications`.
    pub(crate) fn render_chat_setting(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let theme = &*self.theme;
        let c = config(cx);
        let chip = |id: &'static str, label: &'static str, on: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(10.))
                .py(px(3.))
                .map(|d| crate::theme_ext::chip(d, theme, on))
                .cursor_pointer()
                .hover(|s| s.border_color(p.accent).bg(p.hover()))
                .child(label)
        };
        div()
            .flex()
            .gap(px(12.))
            .py(px(7.))
            .child(
                div()
                    .w(px(92.))
                    .flex_none()
                    .pt(px(4.))
                    .text_size(px(10.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.muted)
                    .child("CHAT"),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.))
                            .child(chip("chat-tab-on", "Show #thicket tab", c.tab).on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.set_chat_setting("chat-tab", true, window, cx)
                                }),
                            ))
                            .child(
                                chip("chat-tab-off", "Hide it", !c.tab).on_click(cx.listener(
                                    |this, _, window, cx| {
                                        this.set_chat_setting("chat-tab", false, window, cx)
                                    },
                                )),
                            )
                            .child(
                                chip("chat-notify", "Notify me of mentions", c.notifications)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        let on = !config(cx).notifications;
                                        this.set_chat_setting("chat-notifications", on, window, cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child(format!("#thicket on Libera.Chat. {EXPERIMENTAL}")),
                    ),
            )
            .into_any_element()
    }

    /// Hook (render_reading_body): the chat screen.
    pub(crate) fn render_chat_screen(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        for input in [
            self.chat.compose.as_ref(),
            self.chat.setup.as_ref().map(|s| &s.nick),
            self.chat.setup.as_ref().map(|s| &s.password),
            self.chat.setup.as_ref().map(|s| &s.email),
            self.chat.setup.as_ref().map(|s| &s.code),
        ]
        .into_iter()
        .flatten()
        {
            input.update(cx, |s, _| s.set_editor_style(p.field()));
        }
        let body = if self.chat_needs_setup(cx) {
            self.render_chat_setup(cx)
        } else {
            self.render_chat_room()
        };
        let mut root = div()
            .id("chat-screen")
            .key_context("Chat")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(p.bg)
            .child(self.render_chat_header(cx))
            .child(body);
        if let Some(f) = &self.chat.focus {
            root = root.track_focus(f).on_key_down(cx.listener(
                |this, ev: &KeyDownEvent, window, cx| {
                    let choosing = this
                        .chat
                        .setup
                        .as_ref()
                        .is_some_and(|s| s.step == Step::Choose);
                    if !choosing {
                        return;
                    }
                    match ev.keystroke.key.as_str() {
                        "1" => this.chat_choose(Step::SignIn, window, cx),
                        "2" => this.chat_choose(Step::Register, window, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                },
            ));
        }
        root.into_any_element()
    }

    fn render_chat_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let (dot, text) = match &self.chat.conn {
            Conn::Connected => (p.green, vm::conn_text(&self.chat.conn)),
            Conn::Off if self.chat_needs_setup(cx) => (p.grey, "not set up".into()),
            Conn::Failed(_) => (p.over, vm::conn_text(&self.chat.conn)),
            c => (p.amber, vm::conn_text(c)),
        };
        let (badge_fg, badge_bg) = p.quiet_badge();
        let signed_in = config(cx).nick.filter(|_| !self.chat_needs_setup(cx));
        div()
            .flex_none()
            .px(px(16.))
            .py(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_b_1()
            .border_color(p.line)
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(13.))
                    .child(THICKET),
            )
            .child(
                div()
                    .px(px(5.))
                    .rounded(px(3.))
                    .text_size(px(10.))
                    .text_color(badge_fg)
                    .bg(badge_bg)
                    .child("EXPERIMENTAL"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.))
                    .text_color(p.muted)
                    .child(self.chat.room.topic.clone().unwrap_or_default()),
            )
            .child(
                div()
                    .id("chat-state")
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(div().size(px(6.)).rounded_full().bg(dot))
                    .child(text),
            )
            .when_some(signed_in, |d, nick| {
                d.child(
                    div()
                        .id("chat-sign-out")
                        .text_size(px(11.))
                        .text_color(p.muted)
                        .cursor_pointer()
                        .hover(|s| s.text_color(p.accent))
                        .tooltip(move |_, cx| {
                            cx.new(|_| {
                                reading::Tip(format!(
                                    "Signed in as {nick}. Sign out forgets the password Burrow \
                                     keeps in the Keychain; your Libera.Chat account stays."
                                ))
                            })
                            .into()
                        })
                        .on_click(cx.listener(|this, _, window, cx| this.chat_sign_out(window, cx)))
                        .child("Sign out"),
                )
            })
            .into_any_element()
    }

    fn render_chat_room(&self) -> AnyElement {
        let p = self.palette;
        let lines: Vec<AnyElement> = self
            .chat
            .room
            .log()
            .enumerate()
            .map(|(i, l)| {
                let time = div()
                    .flex_none()
                    .w(px(38.))
                    .text_size(px(10.5))
                    .text_color(p.muted)
                    .child(vm::clock(l.at));
                let row = div()
                    .id(("chat-line", i))
                    .px(px(16.))
                    .py(px(2.))
                    .flex()
                    .gap(px(8.))
                    .items_start()
                    .line_height(relative(1.4));
                match l.kind {
                    LineKind::Membership | LineKind::Status | LineKind::Topic => {
                        let text = match l.kind {
                            LineKind::Membership => format!("{} {}", l.nick, l.text),
                            LineKind::Topic => format!("Topic: {}", l.text),
                            _ => l.text.clone(),
                        };
                        row.text_size(px(11.))
                            .text_color(p.muted)
                            .child(time)
                            .child(div().flex_1().min_w_0().child(text))
                    }
                    _ => {
                        let nick = match l.kind {
                            LineKind::Action => format!("* {}", l.nick),
                            LineKind::Notice => format!("-{}-", l.nick),
                            _ if l.private => format!("{} → you", l.nick),
                            _ => l.nick.clone(),
                        };
                        row.text_size(px(13.))
                            .when(l.mention, |d| {
                                d.bg(p.sel).border_l_2().border_color(p.accent)
                            })
                            .child(time)
                            .child(
                                div()
                                    .flex_none()
                                    .max_w(px(160.))
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(if l.own { p.accent } else { p.ink })
                                    .child(nick),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .when(l.kind == LineKind::Action, |d| d.italic())
                                    .when(l.kind == LineKind::Notice, |d| d.text_color(p.muted))
                                    .child(l.text.clone()),
                            )
                    }
                }
                .into_any_element()
            })
            .collect();
        let empty = self.chat.room.log_len() == 0;
        let members = self.chat.room.members();
        let count = members.len();
        let users: Vec<AnyElement> = members
            .into_iter()
            .enumerate()
            .map(|(i, m)| {
                let op = blyg_irc::is_op(&m.prefixes);
                let sym = m
                    .prefixes
                    .chars()
                    .next()
                    .map(String::from)
                    .unwrap_or_default();
                div()
                    .id(("chat-user", i))
                    .px(px(12.))
                    .py(px(1.))
                    .flex()
                    .gap(px(4.))
                    .text_size(px(12.))
                    .child(
                        div()
                            .w(px(10.))
                            .flex_none()
                            .text_color(if op { p.accent } else { p.muted })
                            .child(sym),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .when(op, |d| d.font_weight(FontWeight::SEMIBOLD))
                            .when(
                                blyg_irc::fold(&m.nick) == blyg_irc::fold(&self.chat.room.me),
                                |d| d.text_color(p.accent),
                            )
                            .child(m.nick.clone()),
                    )
                    .into_any_element()
            })
            .collect();
        let compose = self.chat.compose.clone();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div()
                            .id("chat-log")
                            .flex_1()
                            .min_w_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.chat.scroll)
                            .py(px(8.))
                            .when(empty, |d| {
                                d.child(
                                    div()
                                        .px(px(16.))
                                        .py(px(12.))
                                        .text_color(p.muted)
                                        .line_height(relative(1.45))
                                        .child(format!(
                                            "Nothing said yet. #thicket is a channel for Burrow \
                                             users on Libera.Chat. {EXPERIMENTAL}"
                                        )),
                                )
                            })
                            .children(lines),
                    )
                    .child(
                        div()
                            .id("chat-users")
                            .w(px(170.))
                            .flex_none()
                            .border_l_1()
                            .border_color(p.line)
                            .overflow_y_scroll()
                            .py(px(8.))
                            .child(
                                div()
                                    .px(px(12.))
                                    .pb(px(6.))
                                    .text_size(px(10.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.muted)
                                    .child(format!("{count} HERE")),
                            )
                            .children(users),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .border_t_1()
                    .border_color(p.line)
                    .px(px(16.))
                    .py(px(8.))
                    .when_some(compose, |d, c| {
                        d.child(
                            crate::theme_ext::field_box(div(), &self.theme)
                                .px(px(10.))
                                .py(px(6.))
                                .text_size(px(13.))
                                .child(gpui_kit::base::input::Input::new(&c)),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_chat_setup(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let theme = &*self.theme;
        let Some(s) = &self.chat.setup else {
            return div().flex_1().into_any_element();
        };
        let para = |t: String| {
            div()
                .mt(px(8.))
                .text_color(p.muted)
                .line_height(relative(1.5))
                .child(t)
        };
        let label = |t: &'static str| {
            div()
                .mt(px(12.))
                .mb(px(4.))
                .text_size(px(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.muted)
                .child(t)
        };
        let field = |e: &Entity<InputState>| {
            crate::theme_ext::field_box(div(), theme)
                .px(px(10.))
                .py(px(7.))
                .text_size(px(13.))
                .child(gpui_kit::base::input::Input::new(e))
        };
        let button = |id: &'static str, text: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .mt(px(14.))
                .px(px(12.))
                .py(px(5.))
                .map(|d| crate::theme_ext::chip(d, theme, true))
                .cursor_pointer()
                .hover(|s| s.border_color(p.accent))
                .child(text)
        };
        let back = div()
            .id("chat-setup-back")
            .mt(px(14.))
            .ml(px(12.))
            .text_size(px(12.))
            .text_color(p.muted)
            .cursor_pointer()
            .hover(|s| s.text_color(p.accent))
            .on_click(cx.listener(|this, _, window, cx| this.chat_setup_back(window, cx)))
            .child("Back");
        let submit = |id: &'static str, text: &'static str, cx: &mut Context<Self>| {
            div().flex().items_center().child(
                button(id, text)
                    .on_click(cx.listener(|this, _, window, cx| this.chat_submit(window, cx))),
            )
        };
        let mut col = div()
            .w(px(520.))
            .max_w_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(16.))
                    .child("Chat with other Burrow users in #thicket"),
            )
            .child(para(format!(
                "#thicket is a channel on Libera.Chat, an IRC network. Burrow keeps you in it \
                 from this tab, with a Libera.Chat account. {EXPERIMENTAL} Nothing connects \
                 until you sign in or create an account here."
            )));
        match s.step {
            Step::Choose => {
                col = col
                    .child(
                        div()
                            .mt(px(6.))
                            .flex()
                            .gap(px(8.))
                            .child(
                                button("chat-choose-sign-in", "1  I have a Libera.Chat account")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.chat_choose(Step::SignIn, window, cx)
                                    })),
                            )
                            .child(
                                button("chat-choose-register", "2  Create an account").on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.chat_choose(Step::Register, window, cx)
                                    }),
                                ),
                            ),
                    )
                    .child(para(
                        "Your password goes in the macOS Keychain, never in a file, and after \
                         this Burrow signs in on its own (SASL over TLS, as Libera.Chat \
                         recommends). Settings (⌘,) › Chat hides the tab."
                            .into(),
                    ));
            }
            Step::SignIn => {
                col = col
                    .child(label("NICK (YOUR ACCOUNT NAME)"))
                    .child(field(&s.nick))
                    .child(label("PASSWORD"))
                    .child(field(&s.password))
                    .child(para(
                        "Burrow signs in with SASL and keeps the password only if Libera.Chat \
                         accepts it."
                            .into(),
                    ))
                    .child(submit("chat-submit", "⏎  Sign in", cx).child(back));
            }
            Step::Register => {
                col = col
                    .child(label("NICK"))
                    .child(field(&s.nick))
                    .child(label("PASSWORD"))
                    .child(field(&s.password))
                    .child(label("EMAIL"))
                    .child(field(&s.email))
                    .child(para(
                        "Your nick becomes your account name. Burrow connects as it and sends \
                         NickServ (Libera.Chat's account service) REGISTER with your password \
                         and email, as Libera's guide says to. Use a real email: Libera.Chat \
                         sends a code there, and drops an account that isn't verified within \
                         about a day."
                            .into(),
                    ))
                    .child(submit("chat-submit", "⏎  Create account", cx).child(back));
            }
            Step::Verify => {
                col = col
                    .child(para(
                        "Check your email. Libera.Chat sent a message with a line like \
                         /msg NickServ VERIFY REGISTER <nick> <code>. Paste the code (or the \
                         whole line) here, and Burrow sends it to NickServ."
                            .into(),
                    ))
                    .child(label("CODE"))
                    .child(field(&s.code))
                    .child(submit("chat-submit", "⏎  Verify", cx).child(back));
            }
        }
        if !s.notices.is_empty() {
            col = col.child(label("NICKSERV SAYS")).child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .text_size(px(12.))
                    .children(s.notices.iter().map(|n| div().child(n.clone()))),
            );
        }
        if let Some(b) = s.busy {
            col = col.child(para(b.into()));
        }
        if let Some(e) = &s.error {
            col = col.child(div().mt(px(10.)).text_color(p.over_text()).child(e.clone()));
        }
        div()
            .id("chat-setup")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .px(px(24.))
            .py(px(28.))
            .child(col)
            .into_any_element()
    }
}
