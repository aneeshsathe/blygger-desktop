//! --- chat --- The #thicket tab in a headless window: real keystrokes
//! through GPUI's dispatch, events fed in directly, and setup run end to end
//! against the in-process IRC server on 127.0.0.1 (never a public server).

use std::sync::Arc;
use std::time::{Duration, Instant};

use blyg_core::config::{MemoryTokenStore, TokenStore};
use blyg_core::{Backend, ConfigStore};
use blyg_irc::testserver::{TestServer, VERIFY_CODE};
use blyg_irc::{ChatMessage, Endpoint, Event, Member, MsgKind, Timing, Tls};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::{Conn, Step};
use crate::app::MainView;
use crate::app::reading::View;
use crate::fake::{FakeBackend, Timing as FakeTiming};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

struct Env<'a> {
    view: Entity<MainView>,
    cx: &'a mut VisualTestContext,
    tokens: Arc<MemoryTokenStore>,
}

fn setup_with<'a>(cx: &'a mut TestAppContext, config: &str) -> Env<'a> {
    let prefs = Prefs::from_config(ConfigStore::in_memory(config).config());
    let tokens = Arc::new(MemoryTokenStore::default());
    let t2: Arc<dyn TokenStore> = tokens.clone();
    let config = config.to_string();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(ConfigStore::in_memory(&config), t2, None, cx);
    });
    let fake = Arc::new(FakeBackend::with_timing(FakeTiming::instant()).without_media_cache());
    let backend: Arc<dyn Backend> = fake.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(
            backend,
            Some(fake),
            prefs,
            std::time::Instant::now(),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    Env { view, cx, tokens }
}

fn setup(cx: &mut TestAppContext) -> Env<'_> {
    setup_with(cx, CONNECTED)
}

fn keys(e: &mut Env, k: &str) {
    e.cx.simulate_keystrokes(&crate::keymap::keys(k));
    e.cx.run_until_parked();
}

fn view_is(e: &mut Env) -> View {
    e.view.read_with(e.cx, |v, _| v.reading.view)
}

fn config_text(e: &mut Env) -> String {
    e.cx.update(|_, cx| {
        crate::settings::get(cx)
            .store
            .text()
            .unwrap_or_default()
            .to_string()
    })
}

fn local(e: &mut Env, server: &TestServer) {
    let port = server.port;
    e.view.update(e.cx, |v, _| {
        v.chat.endpoint = Some(Endpoint {
            host: "127.0.0.1".into(),
            port,
            tls: Tls::PlainForTests,
        });
        v.chat.timing = Timing {
            backoff: vec![Duration::from_millis(50)],
            connect_timeout: Duration::from_secs(5),
            idle_ping: Duration::from_secs(60),
            idle_drop: Duration::from_secs(120),
        };
    });
}

/// Let the client's thread work, handing its events to the window, until
/// `f` holds (10 s at most).
fn pump(e: &mut Env, f: impl Fn(&MainView, &gpui_kit::App) -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        e.view.update_in(e.cx, |v, w, cx| v.chat_drain(w, cx));
        e.cx.run_until_parked();
        if e.view.read_with(e.cx, |v, cx| f(v, cx)) {
            return true;
        }
        if Instant::now() > until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn type_into(
    e: &mut Env,
    field: impl Fn(&super::Setup) -> Entity<gpui_kit::base::input::InputState>,
    text: &str,
) {
    let text = text.to_string();
    e.view.update_in(e.cx, |v, w, cx| {
        let f = field(v.chat.setup.as_ref().expect("setup"));
        f.update(cx, |s, cx| s.set_value(text, w, cx));
    });
}

fn feed(e: &mut Env, ev: Event) {
    e.view.update_in(e.cx, |v, w, cx| v.chat_event(ev, w, cx));
}

fn msg(from: &str, target: &str, text: &str) -> Event {
    Event::Message(ChatMessage {
        target: target.into(),
        from: from.into(),
        text: text.into(),
        kind: MsgKind::Message,
        own: false,
    })
}

#[gpui_kit::test]
fn the_tab_opens_setup_and_nothing_connects(cx: &mut TestAppContext) {
    let mut e = setup(cx);
    let has_tab = e.view.update(e.cx, |v, cx| v.render_chat_tab(cx).is_some());
    assert!(has_tab, "on by default");
    assert_eq!(view_is(&mut e), View::Posts);

    keys(&mut e, "cmd-shift-i");
    assert_eq!(view_is(&mut e), View::Chat);
    e.view.read_with(e.cx, |v, cx| {
        assert!(v.chat_needs_setup(cx));
        assert_eq!(v.chat.setup.as_ref().map(|s| s.step), Some(Step::Choose));
        assert!(!v.chat.running(), "nothing connects until setup");
        assert_eq!(v.chat.conn, Conn::Off);
    });

    // 2: create an account; Back; 1: sign in.
    e.cx.simulate_keystrokes("2");
    e.cx.run_until_parked();
    let step = e
        .view
        .read_with(e.cx, |v, _| v.chat.setup.as_ref().map(|s| s.step));
    assert_eq!(step, Some(Step::Register));
    e.view.update_in(e.cx, |v, w, cx| v.chat_setup_back(w, cx));
    e.cx.simulate_keystrokes("1");
    e.cx.run_until_parked();
    let step = e
        .view
        .read_with(e.cx, |v, _| v.chat.setup.as_ref().map(|s| s.step));
    assert_eq!(step, Some(Step::SignIn));
    assert!(!e.view.read_with(e.cx, |v, _| v.chat.running()));

    // Setup's checks stop a bad registration before anything connects.
    e.view
        .update_in(e.cx, |v, w, cx| v.chat_choose(Step::Register, w, cx));
    type_into(&mut e, |s| s.nick.clone(), "ada_example");
    type_into(&mut e, |s| s.password.clone(), "has spaces in it");
    type_into(&mut e, |s| s.email.clone(), "ada@example.com");
    e.view.update_in(e.cx, |v, w, cx| v.chat_submit(w, cx));
    e.view.read_with(e.cx, |v, _| {
        let err = v.chat.setup.as_ref().and_then(|s| s.error.clone()).unwrap();
        assert!(err.contains("without spaces"), "{err}");
        assert!(!v.chat.running());
    });

    // Again: back to Posts.
    keys(&mut e, "cmd-shift-i");
    assert_eq!(view_is(&mut e), View::Posts);
}

#[gpui_kit::test]
fn unread_and_mention_marks_and_a_notification(cx: &mut TestAppContext) {
    let mut e = setup(cx);
    feed(
        &mut e,
        Event::Registered {
            nick: "ada_example".into(),
        },
    );
    feed(
        &mut e,
        Event::Joined {
            channel: "#thicket".into(),
        },
    );
    feed(&mut e, msg("grace_example", "#thicket", "morning all"));
    feed(&mut e, msg("lin_example", "#thicket", "morning"));
    e.view.read_with(e.cx, |v, _| {
        assert_eq!(
            super::vm::mark(v.chat.unread, v.chat.mentions),
            super::vm::Mark::Unread(2)
        );
        assert!(v.chat.notified.is_empty(), "no mention, no notification");
    });

    feed(
        &mut e,
        msg("grace_example", "#thicket", "ada_example: have a look"),
    );
    e.view.read_with(e.cx, |v, _| {
        assert_eq!(
            super::vm::mark(v.chat.unread, v.chat.mentions),
            super::vm::Mark::Mention(1)
        );
        assert_eq!(
            v.chat.notified,
            [(
                "#thicket".to_string(),
                "grace_example: ada_example: have a look".to_string()
            )]
        );
        let last = v.chat.room.log().last().unwrap();
        assert!(last.mention);
    });
    // Joins are quiet: no unread.
    feed(
        &mut e,
        Event::Join {
            channel: "#thicket".into(),
            nick: "cy_example".into(),
        },
    );
    assert_eq!(e.view.read_with(e.cx, |v, _| v.chat.unread), 3);

    // The tab clears its marks when it comes to the front, and gets none
    // while it's there.
    keys(&mut e, "cmd-shift-i");
    e.view.read_with(e.cx, |v, _| {
        assert_eq!((v.chat.unread, v.chat.mentions), (0, 0));
    });
    feed(
        &mut e,
        msg("grace_example", "#thicket", "ada_example again"),
    );
    e.view.read_with(e.cx, |v, _| {
        assert_eq!((v.chat.unread, v.chat.mentions), (0, 0));
    });
}

#[gpui_kit::test]
fn notifications_can_be_turned_off(cx: &mut TestAppContext) {
    let mut e = setup_with(
        cx,
        "blyg-url = https://blyg.example.com\nchat-notifications = false\n",
    );
    feed(
        &mut e,
        Event::Registered {
            nick: "ada_example".into(),
        },
    );
    feed(&mut e, msg("grace_example", "ada_example", "psst"));
    e.view.read_with(e.cx, |v, _| {
        assert_eq!(v.chat.mentions, 1, "a private message is a mention");
        assert!(v.chat.notified.is_empty());
    });
}

#[gpui_kit::test]
fn the_user_list_puts_ops_first(cx: &mut TestAppContext) {
    let mut e = setup(cx);
    feed(
        &mut e,
        Event::Registered {
            nick: "ada_example".into(),
        },
    );
    feed(
        &mut e,
        Event::Joined {
            channel: "#thicket".into(),
        },
    );
    let m = |n: &str, p: &str| Member {
        nick: n.into(),
        prefixes: p.into(),
    };
    feed(
        &mut e,
        Event::Names {
            channel: "#thicket".into(),
            members: vec![
                m("zed_example", ""),
                m("ada_example", ""),
                m("lin_example", "+"),
                m("grace_example", "@"),
            ],
        },
    );
    feed(
        &mut e,
        Event::MemberMode {
            channel: "#thicket".into(),
            nick: "zed_example".into(),
            prefix: '@',
            on: true,
        },
    );
    let list: Vec<(String, bool)> = e.view.read_with(e.cx, |v, _| {
        v.chat
            .room
            .members()
            .iter()
            .map(|m| (m.nick.clone(), blyg_irc::is_op(&m.prefixes)))
            .collect()
    });
    assert_eq!(
        list,
        [
            ("grace_example".to_string(), true),
            ("zed_example".to_string(), true),
            ("lin_example".to_string(), false),
            ("ada_example".to_string(), false),
        ]
    );
}

#[gpui_kit::test]
fn the_tab_can_be_hidden(cx: &mut TestAppContext) {
    let mut e = setup_with(
        cx,
        "blyg-url = https://blyg.example.com\nchat-tab = false\n",
    );
    let has_tab = e.view.update(e.cx, |v, cx| v.render_chat_tab(cx).is_some());
    assert!(!has_tab);
    keys(&mut e, "cmd-shift-i");
    assert_eq!(view_is(&mut e), View::Posts);
    let toast = e
        .view
        .read_with(e.cx, |v, _| v.toast.as_ref().map(|t| t.text.to_string()));
    assert_eq!(toast.as_deref(), Some("The #thicket chat tab is off"));

    // Settings' switch writes the key and brings it back.
    e.view
        .update_in(e.cx, |v, w, cx| v.set_chat_setting("chat-tab", true, w, cx));
    assert!(config_text(&mut e).contains("chat-tab = true"));
    keys(&mut e, "cmd-shift-i");
    assert_eq!(view_is(&mut e), View::Chat);
    e.view.update_in(e.cx, |v, w, cx| {
        v.set_chat_setting("chat-tab", false, w, cx)
    });
    assert_eq!(view_is(&mut e), View::Posts, "hiding it leaves the tab");
}

#[gpui_kit::test]
fn signing_in_keeps_the_password_in_the_keychain_and_joins(cx: &mut TestAppContext) {
    let server = TestServer::start(None);
    server.add_account("ada_example", "correct-horse-7");
    let mut e = setup(cx);
    local(&mut e, &server);
    keys(&mut e, "cmd-shift-i");
    e.cx.simulate_keystrokes("1");
    e.cx.run_until_parked();

    // A wrong password first: nothing is kept.
    type_into(&mut e, |s| s.nick.clone(), "ada_example");
    type_into(&mut e, |s| s.password.clone(), "wrong-password");
    e.view.update_in(e.cx, |v, w, cx| v.chat_submit(w, cx));
    assert!(pump(&mut e, |v, _| v
        .chat
        .setup
        .as_ref()
        .is_some_and(|s| s.error.is_some())));
    assert!(!config_text(&mut e).contains("chat-nick"));
    assert_eq!(e.tokens.get("irc:127.0.0.1:ada_example").unwrap(), None);

    type_into(&mut e, |s| s.password.clone(), "correct-horse-7");
    e.view.update_in(e.cx, |v, w, cx| v.chat_submit(w, cx));
    assert!(pump(&mut e, |v, _| v.chat.room.joined
        && v.chat.room.member_count() == 3));
    assert!(config_text(&mut e).contains("chat-nick = ada_example"));
    assert_eq!(
        e.tokens
            .get("irc:127.0.0.1:ada_example")
            .unwrap()
            .as_deref(),
        Some("correct-horse-7")
    );
    e.view.read_with(e.cx, |v, cx| {
        assert!(!v.chat_needs_setup(cx));
        assert_eq!(v.chat.conn, Conn::Connected);
    });
    assert!(
        !config_text(&mut e).contains("correct-horse-7"),
        "never in the config"
    );

    // Say something, and /me.
    for text in ["hello #thicket", "/me waves"] {
        e.view.update_in(e.cx, |v, w, cx| {
            let c = v.chat.compose.clone().unwrap();
            c.update(cx, |s, cx| s.set_value(text, w, cx));
            v.chat_send(w, cx);
        });
    }
    assert!(server.wait(5, |s| {
        s.received()
            .iter()
            .any(|l| l == "PRIVMSG #thicket :\u{1}ACTION waves\u{1}")
    }));
    assert!(
        server
            .received()
            .iter()
            .any(|l| l == "PRIVMSG #thicket :hello #thicket")
    );

    // Leave the tab; a mention arrives over the wire.
    keys(&mut e, "cmd-shift-i");
    server.say("grace_example", "#thicket", "welcome, ada_example");
    let ok = pump(&mut e, |v, _| v.chat.mentions == 1);
    assert!(
        ok,
        "{:?} {:?} {:?}",
        view_is(&mut e),
        e.view.read_with(e.cx, |v, _| v
            .chat
            .room
            .log()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()),
        e.view.read_with(e.cx, |v, _| (
            v.chat.conn.clone(),
            v.chat.unread,
            v.chat.room.me.clone()
        ))
    );

    // A dropped connection reconnects on its own and joins again.
    server.drop_all();
    assert!(pump(&mut e, |v, _| matches!(
        v.chat.conn,
        Conn::Retrying(_) | Conn::Connecting | Conn::SigningIn
    )));
    assert!(pump(&mut e, |v, _| v.chat.conn == Conn::Connected
        && v.chat.room.joined));
    assert!(server.connections() >= 3);

    // Sign out forgets both.
    e.view.update_in(e.cx, |v, w, cx| v.chat_sign_out(w, cx));
    assert!(!config_text(&mut e).contains("chat-nick"));
    assert_eq!(e.tokens.get("irc:127.0.0.1:ada_example").unwrap(), None);
    assert!(!e.view.read_with(e.cx, |v, _| v.chat.running()));
}

#[gpui_kit::test]
fn creating_an_account_registers_verifies_and_signs_in(cx: &mut TestAppContext) {
    let server = TestServer::start(None);
    let mut e = setup(cx);
    local(&mut e, &server);
    keys(&mut e, "cmd-shift-i");
    e.cx.simulate_keystrokes("2");
    e.cx.run_until_parked();
    type_into(&mut e, |s| s.nick.clone(), "ada_example");
    type_into(&mut e, |s| s.password.clone(), "correct-horse-7");
    type_into(&mut e, |s| s.email.clone(), "ada@example.com");
    e.view.update_in(e.cx, |v, w, cx| v.chat_submit(w, cx));
    assert!(pump(&mut e, |v, _| v.chat.setup.as_ref().is_some_and(
        |s| s.step == Step::Verify && s.notices.iter().any(|n| n.contains("registered"))
    )));
    assert!(
        server
            .received()
            .contains(&"PRIVMSG NickServ :REGISTER correct-horse-7 ada@example.com".to_string())
    );
    assert!(
        !config_text(&mut e).contains("chat-nick"),
        "not until verified"
    );

    // A wrong code: NickServ says so, setup stays.
    type_into(&mut e, |s| s.code.clone(), "nope1234");
    e.view.update_in(e.cx, |v, w, cx| v.chat_submit(w, cx));
    assert!(pump(&mut e, |v, _| v.chat.setup.as_ref().is_some_and(
        |s| s.notices.iter().any(|n| n.contains("Verification failed"))
    )));
    // The emailed line, pasted whole.
    let line = format!("/msg NickServ VERIFY REGISTER ada_example {VERIFY_CODE}");
    type_into(&mut e, |s| s.code.clone(), &line);
    e.view.update_in(e.cx, |v, w, cx| v.chat_submit(w, cx));
    // Verified: kept, then signed in again with SASL and in the channel.
    assert!(pump(&mut e, |v, _| v.chat.setup.is_none() && v.chat.room.joined));
    assert!(config_text(&mut e).contains("chat-nick = ada_example"));
    assert_eq!(
        e.tokens
            .get("irc:127.0.0.1:ada_example")
            .unwrap()
            .as_deref(),
        Some("correct-horse-7")
    );
    assert!(server.received().iter().any(|l| l == "AUTHENTICATE PLAIN"));
}

#[gpui_kit::test]
fn a_set_up_chat_signs_in_on_launch(cx: &mut TestAppContext) {
    // Configured, with the password kept: MainView::new connects. Here the
    // endpoint can only be set after the window opens, so drive the same
    // path (`chat_autoconnect`) the launch takes.
    let server = TestServer::start(None);
    server.add_account("ada_example", "correct-horse-7");
    let mut e = setup_with(
        cx,
        "blyg-url = https://blyg.example.com\nchat-nick = ada_example\n",
    );
    assert!(
        !e.view.read_with(e.cx, |v, _| v.chat.running()),
        "the default server is never dialled in tests"
    );
    local(&mut e, &server);
    e.tokens
        .set("irc:127.0.0.1:ada_example", "correct-horse-7")
        .unwrap();
    e.view.update(e.cx, |v, cx| v.chat_autoconnect(cx));
    assert!(pump(&mut e, |v, _| v.chat.room.joined));
    keys(&mut e, "cmd-shift-i");
    e.view.read_with(e.cx, |v, cx| {
        assert!(!v.chat_needs_setup(cx));
        assert_eq!(v.chat.room.member_count(), 3);
    });
}
