//! The real client (socket thread, TLS, session, reconnects) against the
//! in-process test server on 127.0.0.1. Nothing here touches a public
//! IRC server.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use blyg_irc::testserver::{TestServer, VERIFY_CODE};
use blyg_irc::{Auth, Client, Command, Endpoint, Event, MsgKind, SessionConfig, Timing, Tls};

type Seen = Arc<Mutex<Vec<Event>>>;

fn quick() -> Timing {
    Timing {
        backoff: vec![Duration::from_millis(50), Duration::from_millis(100)],
        connect_timeout: Duration::from_secs(5),
        idle_ping: Duration::from_secs(60),
        idle_drop: Duration::from_secs(120),
    }
}

/// A self-signed certificate for `localhost`, made in the test, and the
/// client and server configs that trust and serve it.
fn tls_pair() -> (Arc<rustls::ClientConfig>, Arc<rustls::ServerConfig>) {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let rcgen::CertifiedKey { cert, key_pair } =
        rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let der = cert.der().clone();
    let key = rustls::pki_types::PrivateKeyDer::try_from(key_pair.serialize_der()).unwrap();
    let server = rustls::ServerConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![der.clone()], key)
        .unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(der).unwrap();
    let client = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    (Arc::new(client), Arc::new(server))
}

fn start(server: &TestServer, tls: Tls, cfg: SessionConfig) -> (Client, Seen) {
    let seen: Seen = Default::default();
    let s2 = seen.clone();
    let ep = Endpoint {
        host: "localhost".into(),
        port: server.port,
        tls,
    };
    let client = Client::start(ep, cfg, quick(), move |e| s2.lock().unwrap().push(e));
    (client, seen)
}

fn sasl(nick: &str, pw: &str) -> SessionConfig {
    SessionConfig::new(
        nick,
        Auth::Sasl {
            account: nick.into(),
            password: pw.into(),
        },
        &["#thicket"],
    )
}

fn has(seen: &Seen, f: impl Fn(&Event) -> bool) -> bool {
    seen.lock().unwrap().iter().any(f)
}

#[test]
fn signs_in_over_tls_joins_chats_and_reconnects() {
    let (client_tls, server_tls) = tls_pair();
    let server = TestServer::start(Some(server_tls));
    server.add_account("ada_example", "correct horse");
    let (client, seen) = start(
        &server,
        Tls::Custom(client_tls),
        sasl("ada_example", "correct horse"),
    );

    assert!(server.wait(10, |_| has(&seen, |e| matches!(e, Event::Names { .. }))));
    assert!(has(
        &seen,
        |e| matches!(e, Event::LoggedIn { account } if account == "ada_example")
    ));
    assert!(has(
        &seen,
        |e| matches!(e, Event::Joined { channel } if channel == "#thicket")
    ));
    let names = seen
        .lock()
        .unwrap()
        .iter()
        .find_map(|e| match e {
            Event::Names { members, .. } => Some(members.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(names.len(), 3);
    assert_eq!(names[0].prefixes, "@");
    let got = server.received();
    assert_eq!(
        &got[..3],
        ["CAP LS 302", "NICK ada_example", "USER burrow 0 * Burrow"]
    );
    assert!(got.contains(&"CAP REQ :sasl".to_string()));
    assert!(got.contains(&"CAP END".to_string()));
    // The server's PING after the welcome was answered.
    assert!(server.wait(5, |s| s.received().iter().any(|l| l == "PONG irc.test")));

    // Someone speaks; we answer.
    server.say("grace_example", "#thicket", "ada_example: welcome!");
    assert!(server.wait(5, |_| has(
        &seen,
        |e| matches!(e, Event::Message(m) if m.from == "grace_example")
    )));
    client.send(Command::Say {
        target: "#thicket".into(),
        text: "thanks!".into(),
    });
    client.send(Command::Me {
        target: "#thicket".into(),
        text: "waves".into(),
    });
    assert!(server.wait(5, |s| {
        s.received()
            .iter()
            .any(|l| l == "PRIVMSG #thicket :\u{1}ACTION waves\u{1}")
    }));
    assert!(has(
        &seen,
        |e| matches!(e, Event::Message(m) if m.own && m.kind == MsgKind::Action)
    ));

    // A dropped connection comes back, signs in and joins again.
    server.drop_all();
    assert!(server.wait(5, |_| has(&seen, |e| matches!(
        e,
        Event::Disconnected {
            retry_in: Some(_),
            ..
        }
    ))));
    assert!(server.wait(10, |s| s.connections() == 2));
    assert!(server.wait(10, |_| {
        seen.lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, Event::Joined { .. }))
            .count()
            == 2
    }));
    client.stop();
    assert!(has(&seen, |e| matches!(
        e,
        Event::Disconnected { retry_in: None, .. }
    )));
}

#[test]
fn a_wrong_password_stops_without_retrying() {
    let server = TestServer::start(None);
    server.add_account("ada_example", "correct horse");
    let (_client, seen) = start(&server, Tls::PlainForTests, sasl("ada_example", "wrong"));
    assert!(server.wait(10, |_| has(&seen, |e| matches!(
        e,
        Event::Disconnected { retry_in: None, .. }
    ))));
    assert!(has(&seen, |e| matches!(e, Event::SaslFailed { .. })));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(server.connections(), 1, "no retry after a refused password");
    assert!(!has(&seen, |e| matches!(e, Event::Registered { .. })));
}

#[test]
fn registers_with_nickserv_then_verifies() {
    let server = TestServer::start(None);
    let mut cfg = SessionConfig::new("ada_example", Auth::None, &[]);
    cfg.alt_nicks = false;
    let (client, seen) = start(&server, Tls::PlainForTests, cfg);
    assert!(server.wait(10, |_| has(&seen, |e| matches!(
        e,
        Event::Registered { .. }
    ))));
    client.send(Command::Register {
        password: "correct-horse-7".into(),
        email: "ada@example.com".into(),
    });
    assert!(server.wait(5, |_| has(&seen, |e| matches!(
        e,
        Event::Message(m) if m.from == "NickServ" && m.text.contains("registered")
    ))));
    client.send(Command::Verify {
        account: "ada_example".into(),
        code: VERIFY_CODE.into(),
    });
    assert!(server.wait(5, |_| has(&seen, |e| matches!(
        e,
        Event::Message(m) if m.text.contains("has now been verified")
    ))));
    assert!(server.has_account("ada_example"));
    // No channel was joined while registering.
    assert!(!server.received().iter().any(|l| l.starts_with("JOIN")));
    client.stop();

    // And the new account signs in.
    let (_c, seen) = start(
        &server,
        Tls::PlainForTests,
        sasl("ada_example", "correct-horse-7"),
    );
    assert!(
        server.wait(10, |_| has(&seen, |e| matches!(e, Event::Joined { .. }))),
        "{:?} {:?}",
        seen.lock().unwrap(),
        server.received()
    );
}

#[test]
fn an_unreachable_server_retries_with_backoff() {
    // Bind and close, so the port refuses.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let seen: Seen = Default::default();
    let s2 = seen.clone();
    let ep = Endpoint {
        host: "127.0.0.1".into(),
        port,
        tls: Tls::PlainForTests,
    };
    let client = Client::start(ep, sasl("ada_example", "x"), quick(), move |e| {
        s2.lock().unwrap().push(e)
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let n = seen
            .lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, Event::Connecting { .. }))
            .count();
        if n >= 3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    client.stop();
    let ev = seen.lock().unwrap().clone();
    let waits: Vec<Duration> = ev
        .iter()
        .filter_map(|e| match e {
            Event::Disconnected {
                retry_in: Some(d), ..
            } => Some(*d),
            _ => None,
        })
        .collect();
    assert!(waits.len() >= 2, "{ev:?}");
    assert_eq!(waits[0], Duration::from_millis(50));
    assert_eq!(waits[1], Duration::from_millis(100));
    assert!(matches!(ev.first(), Some(Event::Connecting { attempt: 0 })));
}
