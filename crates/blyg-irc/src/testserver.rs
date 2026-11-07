//! A tiny in-process IRC server for tests (feature `test-server`): listens
//! on 127.0.0.1, speaks just enough for the real client to run end to end
//! (CAP LS/REQ sasl, AUTHENTICATE PLAIN with 900/903/904, NICK/USER → 001,
//! JOIN with NAMES 353/366, PRIVMSG, NickServ REGISTER/VERIFY notices,
//! PING/PONG) and lets a test speak as other users or drop a connection.
//! Never a public server: nothing in Burrow's tests reaches the network.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;

use crate::message::Message;

pub const NAME: &str = "irc.test";
/// The code NickServ "emails" when an account registers.
pub const VERIFY_CODE: &str = "abcdefgh";

#[derive(Default)]
struct Conn {
    nick: String,
    outbox: Vec<String>,
    drop: bool,
    alive: bool,
}

#[derive(Default)]
struct State {
    /// account → password
    accounts: HashMap<String, String>,
    /// Registered but not verified: account → password.
    pending: HashMap<String, String>,
    /// Every line any client sent, in order (passwords included: tests
    /// only, and only invented ones).
    received: Vec<String>,
    conns: Vec<Arc<Mutex<Conn>>>,
    /// Channel members other than the clients (`@grace_example`).
    regulars: Vec<String>,
}

/// A running test server.
pub struct TestServer {
    pub port: u16,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    accepted: Arc<AtomicUsize>,
}

impl TestServer {
    /// Start one. `tls`: a rustls server config, or `None` for plain TCP.
    pub fn start(tls: Option<Arc<rustls::ServerConfig>>) -> TestServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        listener.set_nonblocking(true).expect("nonblocking");
        let state = Arc::new(Mutex::new(State {
            regulars: vec!["@grace_example".into(), "+lin_example".into()],
            ..Default::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let accepted = Arc::new(AtomicUsize::new(0));
        let (s, st, acc) = (state.clone(), stop.clone(), accepted.clone());
        std::thread::spawn(move || {
            while !st.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((sock, _)) => {
                        acc.fetch_add(1, Ordering::SeqCst);
                        let conn = Arc::new(Mutex::new(Conn {
                            alive: true,
                            ..Default::default()
                        }));
                        s.lock().unwrap().conns.push(conn.clone());
                        let (s, st, tls) = (s.clone(), st.clone(), tls.clone());
                        std::thread::spawn(move || {
                            let _ = serve(sock, tls, s, conn.clone(), st);
                            conn.lock().unwrap().alive = false;
                        });
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });
        TestServer {
            port,
            state,
            stop,
            accepted,
        }
    }

    /// An account that already exists.
    pub fn add_account(&self, account: &str, password: &str) {
        self.state
            .lock()
            .unwrap()
            .accounts
            .insert(account.to_lowercase(), password.to_string());
    }

    pub fn has_account(&self, account: &str) -> bool {
        self.state
            .lock()
            .unwrap()
            .accounts
            .contains_key(&account.to_lowercase())
    }

    /// Every line the clients sent so far.
    pub fn received(&self) -> Vec<String> {
        self.state.lock().unwrap().received.clone()
    }

    /// How many connections were accepted.
    pub fn connections(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }

    /// Send a raw line to every live client.
    pub fn broadcast(&self, line: &str) {
        for c in &self.state.lock().unwrap().conns {
            let mut c = c.lock().unwrap();
            if c.alive {
                c.outbox.push(line.to_string());
            }
        }
    }

    /// `from` says `text` in `channel`.
    pub fn say(&self, from: &str, channel: &str, text: &str) {
        self.broadcast(&format!(":{from}!u@test PRIVMSG {channel} :{text}"));
    }

    /// Drop every live connection (the client should reconnect).
    pub fn drop_all(&self) {
        for c in &self.state.lock().unwrap().conns {
            c.lock().unwrap().drop = true;
        }
    }

    /// Wait until `f` holds (polling), up to `secs` seconds.
    pub fn wait(&self, secs: u64, mut f: impl FnMut(&TestServer) -> bool) -> bool {
        let until = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < until {
            if f(self) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        f(self)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.drop_all();
    }
}

enum Sock {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ServerConnection, TcpStream>>),
}

impl Read for Sock {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        match self {
            Sock::Plain(s) => s.read(b),
            Sock::Tls(s) => s.read(b),
        }
    }
}

impl Write for Sock {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        match self {
            Sock::Plain(s) => s.write(b),
            Sock::Tls(s) => s.write(b),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Sock::Plain(s) => s.flush(),
            Sock::Tls(s) => s.flush(),
        }
    }
}

fn serve(
    sock: TcpStream,
    tls: Option<Arc<rustls::ServerConfig>>,
    state: Arc<Mutex<State>>,
    conn: Arc<Mutex<Conn>>,
    stop: Arc<AtomicBool>,
) -> io::Result<()> {
    sock.set_nonblocking(false)?;
    sock.set_read_timeout(Some(Duration::from_millis(20)))?;
    let mut s = match tls {
        None => Sock::Plain(sock),
        Some(cfg) => {
            let c = rustls::ServerConnection::new(cfg).map_err(io::Error::other)?;
            Sock::Tls(Box::new(rustls::StreamOwned::new(c, sock)))
        }
    };
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut nick = String::new();
    let mut user = false;
    let mut cap_open = false;
    let mut welcomed = false;
    loop {
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        let (out, dropped) = {
            let mut c = conn.lock().unwrap();
            (std::mem::take(&mut c.outbox), c.drop)
        };
        if dropped {
            return Ok(());
        }
        for l in out {
            send(&mut s, &l)?;
        }
        let n = match s.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e),
        };
        buf.extend_from_slice(&chunk[..n]);
        while let Some(i) = buf.iter().position(|&b| b == b'\n') {
            let raw: Vec<u8> = buf.drain(..=i).collect();
            let line = String::from_utf8_lossy(&raw).trim_end().to_string();
            state.lock().unwrap().received.push(line.clone());
            let Ok(m) = Message::parse(&line) else {
                continue;
            };
            let p = |i: usize| m.param(i).unwrap_or("").to_string();
            let me = if nick.is_empty() {
                "*".to_string()
            } else {
                nick.clone()
            };
            match m.command.as_str() {
                "CAP" => match p(0).as_str() {
                    "LS" => {
                        cap_open = true;
                        send(
                            &mut s,
                            &format!(":{NAME} CAP * LS :multi-prefix sasl=PLAIN"),
                        )?;
                    }
                    "REQ" => send(&mut s, &format!(":{NAME} CAP {me} ACK :{}", p(1)))?,
                    "END" => cap_open = false,
                    _ => {}
                },
                "AUTHENTICATE" if p(0) == "PLAIN" => send(&mut s, "AUTHENTICATE +")?,
                "AUTHENTICATE" => {
                    let raw = base64::engine::general_purpose::STANDARD
                        .decode(p(0))
                        .unwrap_or_default();
                    let parts: Vec<String> = String::from_utf8_lossy(&raw)
                        .split('\0')
                        .map(str::to_string)
                        .collect();
                    let ok = parts.len() == 3
                        && state
                            .lock()
                            .unwrap()
                            .accounts
                            .get(&parts[1].to_lowercase())
                            .is_some_and(|pw| *pw == parts[2]);
                    if ok {
                        send(
                            &mut s,
                            &format!(
                                ":{NAME} 900 {me} {me}!u@test {} :You are now logged in as {}",
                                parts[1], parts[1]
                            ),
                        )?;
                        send(
                            &mut s,
                            &format!(":{NAME} 903 {me} :SASL authentication successful"),
                        )?;
                    } else {
                        send(
                            &mut s,
                            &format!(":{NAME} 904 {me} :SASL authentication failed"),
                        )?;
                    }
                }
                "NICK" => {
                    nick = p(0);
                    conn.lock().unwrap().nick = nick.clone();
                }
                "USER" => user = true,
                "PING" => send(&mut s, &format!(":{NAME} PONG {NAME} :{}", p(0)))?,
                "PONG" => {}
                "JOIN" => {
                    let ch = p(0);
                    send(&mut s, &format!(":{nick}!u@test JOIN {ch}"))?;
                    send(
                        &mut s,
                        &format!(":{NAME} 332 {nick} {ch} :Burrow users, quotes and stubs"),
                    )?;
                    let regulars = state.lock().unwrap().regulars.join(" ");
                    send(
                        &mut s,
                        &format!(":{NAME} 353 {nick} = {ch} :{regulars} {nick}"),
                    )?;
                    send(
                        &mut s,
                        &format!(":{NAME} 366 {nick} {ch} :End of /NAMES list."),
                    )?;
                }
                "PRIVMSG" if p(0).eq_ignore_ascii_case("NickServ") => {
                    let words: Vec<String> = p(1).split(' ').map(str::to_string).collect();
                    let w = |i: usize| words.get(i).cloned().unwrap_or_default();
                    let ns = |s: &mut Sock, t: &str| {
                        send(
                            s,
                            &format!(":NickServ!NickServ@services.test NOTICE {nick} :{t}"),
                        )
                    };
                    match w(0).to_ascii_uppercase().as_str() {
                        "REGISTER" => {
                            state
                                .lock()
                                .unwrap()
                                .pending
                                .insert(nick.to_lowercase(), w(1));
                            ns(
                                &mut s,
                                &format!(
                                    "An email containing nickname activation instructions has been sent to {}.",
                                    w(2)
                                ),
                            )?;
                            ns(
                                &mut s,
                                "If you do not complete registration within one day, your nickname will expire.",
                            )?;
                            ns(&mut s, &format!("{nick} is now registered to {}.", w(2)))?;
                        }
                        "VERIFY" => {
                            let acct = w(2).to_lowercase();
                            let mut st = state.lock().unwrap();
                            match st.pending.remove(&acct) {
                                Some(pw) if w(3) == VERIFY_CODE => {
                                    st.accounts.insert(acct, pw);
                                    drop(st);
                                    ns(&mut s, &format!("{} has now been verified.", w(2)))?;
                                }
                                other => {
                                    if let Some(pw) = other {
                                        st.pending.insert(acct, pw);
                                    }
                                    drop(st);
                                    ns(
                                        &mut s,
                                        &format!("Verification failed. Invalid key for {}.", w(2)),
                                    )?;
                                }
                            }
                        }
                        _ => ns(&mut s, "Unknown command.")?,
                    }
                }
                "PRIVMSG" | "NOTICE" => {}
                "QUIT" => return Ok(()),
                _ => {}
            }
            if !welcomed && !cap_open && user && !nick.is_empty() {
                welcomed = true;
                send(
                    &mut s,
                    &format!(":{NAME} 001 {nick} :Welcome to the test network, {nick}"),
                )?;
                send(
                    &mut s,
                    &format!(
                        ":{NAME} 005 {nick} PREFIX=(ov)@+ CHANMODES=b,k,l,imnst :are supported by this server"
                    ),
                )?;
                send(&mut s, &format!("PING :{NAME}"))?;
            }
        }
    }
}

fn send(s: &mut Sock, line: &str) -> io::Result<()> {
    s.write_all(format!("{line}\r\n").as_bytes())?;
    s.flush()
}
