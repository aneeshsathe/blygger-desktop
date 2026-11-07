//! One connection's protocol state, with no I/O: feed it the server's
//! messages, send what it returns. Registration (CAP LS 302 → CAP REQ sasl →
//! AUTHENTICATE PLAIN → 903 → CAP END), the welcome, joins, NAMES, PING, and
//! the events the UI cares about. `conn` runs it over a socket; the tests
//! run it over scripted transcripts.

use std::collections::HashMap;

use base64::Engine as _;

use crate::message::{Message, ctcp, fold, strip_formatting};

/// How to log in.
#[derive(Clone, PartialEq, Eq)]
pub enum Auth {
    /// No account (a first connection, to register one).
    None,
    /// SASL PLAIN with an account name and its password.
    Sasl { account: String, password: String },
}

impl std::fmt::Debug for Auth {
    // Never print the password.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Auth::None => write!(f, "None"),
            Auth::Sasl { account, .. } => write!(f, "Sasl {{ account: {account:?}, .. }}"),
        }
    }
}

/// What a session needs to know.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub nick: String,
    /// The ident (`USER <user> 0 * :<realname>`).
    pub user: String,
    pub realname: String,
    pub auth: Auth,
    /// Joined after the welcome (001), and again after a reconnect.
    pub channels: Vec<String>,
    /// When the nick is taken before the welcome, try `nick_`, `nick__`…
    /// (off while registering: the account is the nick).
    pub alt_nicks: bool,
}

impl SessionConfig {
    pub fn new(nick: &str, auth: Auth, channels: &[&str]) -> SessionConfig {
        SessionConfig {
            nick: nick.to_string(),
            user: "burrow".into(),
            realname: "Burrow".into(),
            auth,
            channels: channels.iter().map(|c| c.to_string()).collect(),
            alt_nicks: true,
        }
    }
}

/// A member of a channel, as NAMES lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub nick: String,
    /// Prefix symbols, highest first (`@`, `+`, `@+`; empty for none).
    pub prefixes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgKind {
    Message,
    /// `/me` (CTCP ACTION).
    Action,
    Notice,
}

/// A PRIVMSG or NOTICE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    /// A channel, or our own nick for a private message (`*` before the
    /// welcome).
    pub target: String,
    /// The sender's nick (or the server's name).
    pub from: String,
    /// Formatting codes stripped.
    pub text: String,
    pub kind: MsgKind,
    /// Sent by us (echoed locally).
    pub own: bool,
}

/// What happened, for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    // From the connection (`conn`), not the session.
    /// Connecting, attempt `n` (0 the first).
    Connecting {
        attempt: u32,
    },
    /// The socket (and TLS) is up; registration starts.
    Connected,
    /// The connection ended; `retry_in` is when it tries again (`None`: it
    /// won't, e.g. the password was refused or it was stopped).
    Disconnected {
        reason: String,
        retry_in: Option<std::time::Duration>,
    },
    // From the session.
    /// The server welcomed us (001) as `nick`.
    Registered {
        nick: String,
    },
    /// SASL succeeded (900): logged in to `account`.
    LoggedIn {
        account: String,
    },
    /// SASL failed (904, 905, 902…), or the server offers no SASL.
    SaslFailed {
        reason: String,
    },
    /// The nick is taken, or not allowed (433, 432) before the welcome.
    NickRejected {
        nick: String,
        reason: String,
    },
    /// We joined `channel`.
    Joined {
        channel: String,
    },
    /// Every member of `channel` (after 366).
    Names {
        channel: String,
        members: Vec<Member>,
    },
    Join {
        channel: String,
        nick: String,
    },
    Part {
        channel: String,
        nick: String,
        reason: String,
    },
    Quit {
        nick: String,
        reason: String,
    },
    Kick {
        channel: String,
        nick: String,
        by: String,
        reason: String,
    },
    /// Someone (perhaps us) changed nick.
    Nick {
        old: String,
        new: String,
    },
    /// A member's prefix (`@`, `+`…) given or taken.
    MemberMode {
        channel: String,
        nick: String,
        prefix: char,
        on: bool,
    },
    Topic {
        channel: String,
        topic: String,
    },
    Message(ChatMessage),
    /// An error numeric or ERROR line worth showing (text only).
    ServerNotice {
        text: String,
    },
}

/// What the session wants done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    Send(String),
    Event(Event),
}

/// Something the user asked for.
#[derive(Clone, PartialEq, Eq)]
pub enum Command {
    /// A message to a channel or nick (long text is split).
    Say {
        target: String,
        text: String,
    },
    /// `/me`.
    Me {
        target: String,
        text: String,
    },
    /// `NickServ REGISTER <password> <email>`.
    Register {
        password: String,
        email: String,
    },
    /// `NickServ VERIFY REGISTER <account> <code>`.
    Verify {
        account: String,
        code: String,
    },
    Join {
        channel: String,
    },
    Quit {
        reason: String,
    },
}

impl std::fmt::Debug for Command {
    // Never print the password.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::Register { .. } => write!(f, "Register {{ .. }}"),
            Command::Say { target, .. } => write!(f, "Say {{ target: {target:?}, .. }}"),
            Command::Me { target, .. } => write!(f, "Me {{ target: {target:?}, .. }}"),
            Command::Verify { account, .. } => write!(f, "Verify {{ account: {account:?}, .. }}"),
            Command::Join { channel } => write!(f, "Join {{ channel: {channel:?} }}"),
            Command::Quit { .. } => write!(f, "Quit"),
        }
    }
}

/// Where registration is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// CAP LS sent, collecting the list.
    CapLs,
    /// CAP REQ sasl sent.
    CapReq,
    /// AUTHENTICATE PLAIN sent, waiting for `+`.
    AuthStart,
    /// Credentials sent, waiting for 903/904.
    AuthSent,
    /// CAP END sent; waiting for 001.
    Registering,
    /// Welcomed.
    Ready,
}

/// The longest text in one PRIVMSG, in bytes: 512 minus the command, the
/// target and a generous allowance for the prefix the server adds.
const MAX_TEXT: usize = 400;

pub struct Session {
    cfg: SessionConfig,
    nick: String,
    phase: Phase,
    caps: Vec<String>,
    /// NAMES replies being collected, by folded channel.
    names: HashMap<String, Vec<Member>>,
    /// ISUPPORT PREFIX: mode letters and their symbols, highest first.
    prefix_modes: Vec<(char, char)>,
    /// ISUPPORT CHANMODES type A, B and C letters (they take a parameter).
    param_modes: (String, String, String),
    tries: u32,
    /// Set when the session should end and not come back (SASL refused).
    fatal: Option<String>,
}

impl Session {
    pub fn new(cfg: SessionConfig) -> Session {
        Session {
            nick: cfg.nick.clone(),
            cfg,
            phase: Phase::CapLs,
            caps: Vec::new(),
            names: HashMap::new(),
            prefix_modes: vec![('o', '@'), ('v', '+')],
            param_modes: ("beIq".into(), "k".into(), "flj".into()),
            tries: 0,
            fatal: None,
        }
    }

    pub fn nick(&self) -> &str {
        &self.nick
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn registered(&self) -> bool {
        self.phase == Phase::Ready
    }

    /// Why this session must not reconnect, if it mustn't.
    pub fn fatal(&self) -> Option<&str> {
        self.fatal.as_deref()
    }

    /// The prefix symbols, highest first (`~&@%+` on Libera's `(qaohv)`).
    pub fn prefix_symbols(&self) -> String {
        self.prefix_modes.iter().map(|(_, s)| *s).collect()
    }

    /// The lines that open a connection.
    pub fn start(&mut self) -> Vec<String> {
        self.phase = Phase::CapLs;
        self.nick = self.cfg.nick.clone();
        self.tries = 0;
        vec![
            "CAP LS 302".into(),
            Message::new("NICK", &[&self.cfg.nick]).to_line(),
            Message::new("USER", &[&self.cfg.user, "0", "*", &self.cfg.realname]).to_line(),
        ]
    }

    /// React to one message from the server.
    pub fn handle(&mut self, m: &Message) -> Vec<Output> {
        let mut out = Vec::new();
        let send = |out: &mut Vec<Output>, s: String| out.push(Output::Send(s));
        let ev = |out: &mut Vec<Output>, e: Event| out.push(Output::Event(e));
        let from = m.source().unwrap_or("").to_string();
        match m.command.as_str() {
            "PING" => send(
                &mut out,
                Message::new("PONG", &[m.param(0).unwrap_or("")]).to_line(),
            ),
            "CAP" => self.on_cap(m, &mut out),
            "AUTHENTICATE" => {
                if m.param(0) == Some("+")
                    && self.phase == Phase::AuthStart
                    && let Auth::Sasl { account, password } = &self.cfg.auth
                {
                    for line in sasl_plain(account, password) {
                        send(&mut out, line);
                    }
                    self.phase = Phase::AuthSent;
                }
            }
            "900" => ev(
                &mut out,
                Event::LoggedIn {
                    account: m.param(2).unwrap_or(&self.nick).to_string(),
                },
            ),
            "903" => {
                send(&mut out, "CAP END".into());
                self.phase = Phase::Registering;
            }
            "902" | "904" | "905" | "906" | "908" => {
                if self.phase == Phase::Ready || self.fatal.is_some() {
                    return out;
                }
                let reason = match m.command.as_str() {
                    "904" => "Libera.Chat didn't accept that account name and password.".into(),
                    "906" => "Signing in was cancelled.".into(),
                    _ => m.trailing().unwrap_or("Signing in failed.").to_string(),
                };
                self.fail(reason, &mut out);
            }
            "001" => {
                if let Some(n) = m.param(0) {
                    self.nick = n.to_string();
                }
                self.phase = Phase::Ready;
                ev(
                    &mut out,
                    Event::Registered {
                        nick: self.nick.clone(),
                    },
                );
                for c in self.cfg.channels.clone() {
                    send(&mut out, Message::new("JOIN", &[&c]).to_line());
                }
            }
            "005" => self.on_isupport(m),
            "432" | "433" | "436" | "437" if self.phase != Phase::Ready => {
                let nick = m.param(1).unwrap_or(&self.nick).to_string();
                let reason = m.trailing().unwrap_or("").to_string();
                ev(
                    &mut out,
                    Event::NickRejected {
                        nick: nick.clone(),
                        reason: reason.clone(),
                    },
                );
                if self.cfg.alt_nicks && m.command != "432" && self.tries < 3 {
                    self.tries += 1;
                    self.nick = format!("{}{}", self.cfg.nick, "_".repeat(self.tries as usize));
                    send(&mut out, Message::new("NICK", &[&self.nick]).to_line());
                } else {
                    let why = if m.command == "432" {
                        format!("{nick} isn't a nick Libera.Chat allows.")
                    } else {
                        format!("{nick} is in use. Pick another nick.")
                    };
                    self.fail(why, &mut out);
                }
            }
            "JOIN" => {
                let channel = m.param(0).unwrap_or("").to_string();
                if self.is_me(&from) {
                    ev(&mut out, Event::Joined { channel });
                } else {
                    ev(
                        &mut out,
                        Event::Join {
                            channel,
                            nick: from,
                        },
                    );
                }
            }
            "PART" => ev(
                &mut out,
                Event::Part {
                    channel: m.param(0).unwrap_or("").to_string(),
                    nick: from,
                    reason: m.param(1).unwrap_or("").to_string(),
                },
            ),
            "QUIT" => ev(
                &mut out,
                Event::Quit {
                    nick: from,
                    reason: m.param(0).unwrap_or("").to_string(),
                },
            ),
            "KICK" => ev(
                &mut out,
                Event::Kick {
                    channel: m.param(0).unwrap_or("").to_string(),
                    nick: m.param(1).unwrap_or("").to_string(),
                    by: from,
                    reason: m.param(2).unwrap_or("").to_string(),
                },
            ),
            "NICK" => {
                let new = m.param(0).unwrap_or("").to_string();
                if self.is_me(&from) {
                    self.nick = new.clone();
                }
                ev(&mut out, Event::Nick { old: from, new });
            }
            "MODE" => self.on_mode(m, &mut out),
            "TOPIC" => ev(
                &mut out,
                Event::Topic {
                    channel: m.param(0).unwrap_or("").to_string(),
                    topic: strip_formatting(m.param(1).unwrap_or("")),
                },
            ),
            "332" => ev(
                &mut out,
                Event::Topic {
                    channel: m.param(1).unwrap_or("").to_string(),
                    topic: strip_formatting(m.param(2).unwrap_or("")),
                },
            ),
            "353" => {
                // 353 <me> <=|*|@> <channel> :<prefixed nicks>
                let channel = fold(m.param(2).unwrap_or(""));
                let symbols = self.prefix_symbols();
                let list = self.names.entry(channel).or_default();
                for word in m.trailing().unwrap_or("").split(' ') {
                    let word = word.trim();
                    if word.is_empty() {
                        continue;
                    }
                    let nick = word.trim_start_matches(|c| symbols.contains(c));
                    let prefixes = word[..word.len() - nick.len()].to_string();
                    // userhost-in-names isn't requested, but be tolerant.
                    let nick = nick.split('!').next().unwrap_or(nick).to_string();
                    list.push(Member { nick, prefixes });
                }
            }
            "366" => {
                let name = m.param(1).unwrap_or("").to_string();
                let members = self.names.remove(&fold(&name)).unwrap_or_default();
                ev(
                    &mut out,
                    Event::Names {
                        channel: name,
                        members,
                    },
                );
            }
            "PRIVMSG" | "NOTICE" => {
                let target = m.param(0).unwrap_or("").to_string();
                let raw = m.param(1).unwrap_or("");
                let mut kind = if m.command == "NOTICE" {
                    MsgKind::Notice
                } else {
                    MsgKind::Message
                };
                let mut text = raw;
                if let Some((cmd, arg)) = ctcp(raw) {
                    if cmd.eq_ignore_ascii_case("ACTION") && kind == MsgKind::Message {
                        kind = MsgKind::Action;
                        text = arg;
                    } else {
                        // Other CTCP (VERSION, PING…): not shown, not answered.
                        return out;
                    }
                }
                ev(
                    &mut out,
                    Event::Message(ChatMessage {
                        target,
                        from,
                        text: strip_formatting(text),
                        kind,
                        own: false,
                    }),
                );
            }
            "ERROR" => ev(
                &mut out,
                Event::ServerNotice {
                    text: m.trailing().unwrap_or("").to_string(),
                },
            ),
            // Join failures and the like (403 no such channel, 471–477, 489).
            "403" | "404" | "405" | "471" | "473" | "474" | "475" | "477" | "489" => ev(
                &mut out,
                Event::ServerNotice {
                    text: m.trailing().unwrap_or("").to_string(),
                },
            ),
            _ => {}
        }
        out
    }

    /// Lines for something the user asked, plus our own messages as events
    /// (the server doesn't echo them).
    pub fn command(&mut self, c: &Command) -> Vec<Output> {
        let mut out = Vec::new();
        match c {
            Command::Say { target, text } => {
                for chunk in split_text(text, MAX_TEXT) {
                    out.push(Output::Send(
                        Message::new("PRIVMSG", &[target, &chunk]).to_line(),
                    ));
                    out.push(Output::Event(Event::Message(ChatMessage {
                        target: target.clone(),
                        from: self.nick.clone(),
                        text: chunk,
                        kind: MsgKind::Message,
                        own: true,
                    })));
                }
            }
            Command::Me { target, text } => {
                let text: String = split_text(text, MAX_TEXT - 9)
                    .into_iter()
                    .next()
                    .unwrap_or_default();
                out.push(Output::Send(
                    Message::new("PRIVMSG", &[target, &format!("\u{1}ACTION {text}\u{1}")])
                        .to_line(),
                ));
                out.push(Output::Event(Event::Message(ChatMessage {
                    target: target.clone(),
                    from: self.nick.clone(),
                    text,
                    kind: MsgKind::Action,
                    own: true,
                })));
            }
            Command::Register { password, email } => out.push(Output::Send(
                Message::new(
                    "PRIVMSG",
                    &["NickServ", &format!("REGISTER {password} {email}")],
                )
                .to_line(),
            )),
            Command::Verify { account, code } => out.push(Output::Send(
                Message::new(
                    "PRIVMSG",
                    &["NickServ", &format!("VERIFY REGISTER {account} {code}")],
                )
                .to_line(),
            )),
            Command::Join { channel } => {
                if !self.cfg.channels.iter().any(|c| fold(c) == fold(channel)) {
                    self.cfg.channels.push(channel.clone());
                }
                out.push(Output::Send(Message::new("JOIN", &[channel]).to_line()));
            }
            Command::Quit { reason } => {
                out.push(Output::Send(Message::new("QUIT", &[reason]).to_line()))
            }
        }
        out
    }

    fn is_me(&self, nick: &str) -> bool {
        fold(nick) == fold(&self.nick)
    }

    fn fail(&mut self, reason: String, out: &mut Vec<Output>) {
        self.fatal = Some(reason.clone());
        if self.phase != Phase::Ready {
            out.push(Output::Event(Event::SaslFailed {
                reason: reason.clone(),
            }));
        }
        out.push(Output::Send("QUIT :".into()));
    }

    fn on_cap(&mut self, m: &Message, out: &mut Vec<Output>) {
        // CAP <target> <sub> [*] :<caps>
        let sub = m.param(1).unwrap_or("").to_ascii_uppercase();
        let more = m.params.len() > 3 && m.param(2) == Some("*");
        let list = m.trailing().unwrap_or("");
        match sub.as_str() {
            "LS" if self.phase == Phase::CapLs => {
                self.caps.extend(
                    list.split(' ')
                        .filter(|c| !c.is_empty())
                        .map(|c| c.split('=').next().unwrap_or(c).to_ascii_lowercase()),
                );
                if more {
                    return;
                }
                let wants_sasl = matches!(self.cfg.auth, Auth::Sasl { .. });
                if wants_sasl && self.caps.iter().any(|c| c == "sasl") {
                    out.push(Output::Send("CAP REQ :sasl".into()));
                    self.phase = Phase::CapReq;
                } else if wants_sasl {
                    self.fail("This server doesn't offer SASL sign-in.".into(), out);
                } else {
                    out.push(Output::Send("CAP END".into()));
                    self.phase = Phase::Registering;
                }
            }
            "ACK" if self.phase == Phase::CapReq => {
                if list.split(' ').any(|c| c.eq_ignore_ascii_case("sasl")) {
                    out.push(Output::Send("AUTHENTICATE PLAIN".into()));
                    self.phase = Phase::AuthStart;
                }
            }
            "NAK" if self.phase == Phase::CapReq => {
                self.fail("The server refused SASL sign-in.".into(), out);
            }
            _ => {}
        }
    }

    fn on_isupport(&mut self, m: &Message) {
        let n = m.params.len();
        for tok in m.params.iter().take(n.saturating_sub(1)).skip(1) {
            if let Some(v) = tok.strip_prefix("PREFIX=") {
                // (qaohv)~&@%+
                if let Some((modes, syms)) = v.strip_prefix('(').and_then(|v| v.split_once(')'))
                    && modes.chars().count() == syms.chars().count()
                    && !modes.is_empty()
                {
                    self.prefix_modes = modes.chars().zip(syms.chars()).collect();
                }
            } else if let Some(v) = tok.strip_prefix("CHANMODES=") {
                let mut parts = v.split(',');
                self.param_modes = (
                    parts.next().unwrap_or("").into(),
                    parts.next().unwrap_or("").into(),
                    parts.next().unwrap_or("").into(),
                );
            }
        }
    }

    fn on_mode(&mut self, m: &Message, out: &mut Vec<Output>) {
        let channel = m.param(0).unwrap_or("");
        if !channel.starts_with(['#', '&', '!', '+']) {
            return; // a user mode
        }
        let Some(modes) = m.param(1) else { return };
        let mut args = m.params.iter().skip(2);
        let mut on = true;
        for c in modes.chars() {
            match c {
                '+' => on = true,
                '-' => on = false,
                c => {
                    if let Some((_, sym)) = self.prefix_modes.iter().find(|(l, _)| *l == c) {
                        if let Some(nick) = args.next() {
                            out.push(Output::Event(Event::MemberMode {
                                channel: channel.to_string(),
                                nick: nick.clone(),
                                prefix: *sym,
                                on,
                            }));
                        }
                    } else if self.param_modes.0.contains(c)
                        || self.param_modes.1.contains(c)
                        || (on && self.param_modes.2.contains(c))
                    {
                        args.next();
                    }
                }
            }
        }
    }
}

/// `AUTHENTICATE` lines carrying `account\0account\0password`, base64, in
/// 400-byte pieces (and a `+` after an exact multiple of 400).
pub fn sasl_plain(account: &str, password: &str) -> Vec<String> {
    let raw = format!("{account}\0{account}\0{password}");
    let b64 = base64::engine::general_purpose::STANDARD.encode(raw.as_bytes());
    let mut out: Vec<String> = b64
        .as_bytes()
        .chunks(400)
        .map(|c| format!("AUTHENTICATE {}", String::from_utf8_lossy(c)))
        .collect();
    if b64.len() % 400 == 0 {
        out.push("AUTHENTICATE +".into());
    }
    out
}

/// `text` in pieces of at most `max` bytes, split at spaces where it can and
/// never inside a character. Newlines start a new piece.
pub fn split_text(text: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut rest = line.trim_end();
        while rest.len() > max {
            let mut cut = max;
            while !rest.is_char_boundary(cut) {
                cut -= 1;
            }
            let at = rest[..cut]
                .rfind(' ')
                .filter(|&i| i > max / 2)
                .unwrap_or(cut);
            out.push(rest[..at].to_string());
            rest = rest[at..].trim_start();
        }
        if !rest.is_empty() {
            out.push(rest.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sasl() -> SessionConfig {
        SessionConfig::new(
            "ada_example",
            Auth::Sasl {
                account: "ada_example".into(),
                password: "correct horse".into(),
            },
            &["#thicket"],
        )
    }

    /// Run a transcript: each server line, then what the session sent and
    /// what it reported.
    fn feed(s: &mut Session, lines: &[&str]) -> (Vec<String>, Vec<Event>) {
        let mut sent = Vec::new();
        let mut events = Vec::new();
        for l in lines {
            for o in s.handle(&Message::parse(l).unwrap()) {
                match o {
                    Output::Send(x) => sent.push(x),
                    Output::Event(e) => events.push(e),
                }
            }
        }
        (sent, events)
    }

    #[test]
    fn sasl_plain_registration_transcript() {
        let mut s = Session::new(sasl());
        assert_eq!(
            s.start(),
            ["CAP LS 302", "NICK ada_example", "USER burrow 0 * Burrow"]
        );
        let (sent, _) = feed(
            &mut s,
            &[
                ":irc.example.org CAP * LS * :account-notify away-notify",
                ":irc.example.org CAP * LS :sasl=PLAIN,EXTERNAL multi-prefix",
            ],
        );
        assert_eq!(sent, ["CAP REQ :sasl"]);
        let (sent, _) = feed(&mut s, &[":irc.example.org CAP * ACK :sasl"]);
        assert_eq!(sent, ["AUTHENTICATE PLAIN"]);
        let (sent, _) = feed(&mut s, &["AUTHENTICATE +"]);
        // base64("ada_example\0ada_example\0correct horse")
        assert_eq!(
            sent,
            ["AUTHENTICATE YWRhX2V4YW1wbGUAYWRhX2V4YW1wbGUAY29ycmVjdCBob3JzZQ=="]
        );
        let (sent, ev) = feed(
            &mut s,
            &[
                ":irc.example.org 900 ada_example ada_example!b@h ada_example :You are now logged in as ada_example",
                ":irc.example.org 903 ada_example :SASL authentication successful",
            ],
        );
        assert_eq!(sent, ["CAP END"]);
        assert_eq!(
            ev,
            [Event::LoggedIn {
                account: "ada_example".into()
            }]
        );
        let (sent, ev) = feed(
            &mut s,
            &[
                ":irc.example.org 001 ada_example :Welcome",
                ":irc.example.org 005 ada_example CHANTYPES=# PREFIX=(qaohv)~&@%+ CHANMODES=eIbq,k,flj,CFLMPQRSTcgimnprstuz :are supported",
            ],
        );
        assert_eq!(sent, ["JOIN #thicket"]);
        assert_eq!(
            ev,
            [Event::Registered {
                nick: "ada_example".into()
            }]
        );
        assert!(s.registered());
        assert_eq!(s.prefix_symbols(), "~&@%+");
    }

    #[test]
    fn sasl_failure_is_fatal_and_quits() {
        let mut s = Session::new(sasl());
        s.start();
        feed(
            &mut s,
            &["CAP * LS :sasl", "CAP * ACK :sasl", "AUTHENTICATE +"],
        );
        let (sent, ev) = feed(
            &mut s,
            &[":srv 904 ada_example :SASL authentication failed"],
        );
        assert_eq!(sent, ["QUIT :"]);
        assert!(matches!(&ev[..], [Event::SaslFailed { .. }]));
        assert!(s.fatal().is_some());
    }

    #[test]
    fn no_sasl_on_the_server_is_fatal_when_signing_in() {
        let mut s = Session::new(sasl());
        s.start();
        let (sent, ev) = feed(&mut s, &["CAP * LS :multi-prefix"]);
        assert_eq!(sent, ["QUIT :"]);
        assert!(matches!(&ev[..], [Event::SaslFailed { .. }]));
    }

    #[test]
    fn without_auth_it_ends_cap_at_once_and_registers() {
        let mut cfg = SessionConfig::new("ada_example", Auth::None, &[]);
        cfg.alt_nicks = false;
        let mut s = Session::new(cfg);
        s.start();
        let (sent, _) = feed(&mut s, &["CAP * LS :sasl multi-prefix"]);
        assert_eq!(sent, ["CAP END"]);
        let (sent, ev) = feed(&mut s, &[":srv 001 ada_example :hi"]);
        assert!(sent.is_empty(), "no channels while registering");
        assert_eq!(ev.len(), 1);
        let out = s.command(&Command::Register {
            password: "hunter22".into(),
            email: "ada@example.com".into(),
        });
        assert_eq!(
            out,
            [Output::Send(
                "PRIVMSG NickServ :REGISTER hunter22 ada@example.com".into()
            )]
        );
        let out = s.command(&Command::Verify {
            account: "ada_example".into(),
            code: "abcdefgh".into(),
        });
        assert_eq!(
            out,
            [Output::Send(
                "PRIVMSG NickServ :VERIFY REGISTER ada_example abcdefgh".into()
            )]
        );
    }

    #[test]
    fn nick_in_use_tries_alternatives_then_registering_refuses() {
        let mut s = Session::new(sasl());
        s.start();
        let (sent, ev) = feed(
            &mut s,
            &[":srv 433 * ada_example :Nickname is already in use"],
        );
        assert_eq!(sent, ["NICK ada_example_"]);
        assert!(matches!(&ev[..], [Event::NickRejected { .. }]));
        assert_eq!(s.nick(), "ada_example_");

        let mut cfg = SessionConfig::new("ada_example", Auth::None, &[]);
        cfg.alt_nicks = false;
        let mut s = Session::new(cfg);
        s.start();
        let (sent, ev) = feed(&mut s, &[":srv 433 * ada_example :in use"]);
        assert_eq!(sent, ["QUIT :"]);
        assert!(matches!(
            &ev[..],
            [Event::NickRejected { .. }, Event::SaslFailed { reason }] if reason.contains("in use")
        ));
    }

    #[test]
    fn ping_pong() {
        let mut s = Session::new(sasl());
        let (sent, _) = feed(&mut s, &["PING :irc.example.org", "PING tok"]);
        assert_eq!(sent, ["PONG irc.example.org", "PONG tok"]);
    }

    #[test]
    fn names_join_part_quit_kick_nick_mode() {
        let mut s = Session::new(sasl());
        s.start();
        feed(&mut s, &[":srv 001 ada_example :hi"]);
        let (_, ev) = feed(
            &mut s,
            &[
                ":ada_example!b@h JOIN #thicket",
                ":srv 332 ada_example #thicket :\u{2}Quotes\u{2} and stubs",
                ":srv 353 ada_example = #thicket :@grace_example +lin_example",
                ":srv 353 ada_example = #thicket :ada_example @+bo_example",
                ":srv 366 ada_example #thicket :End of /NAMES list.",
            ],
        );
        assert_eq!(
            ev,
            [
                Event::Joined {
                    channel: "#thicket".into()
                },
                Event::Topic {
                    channel: "#thicket".into(),
                    topic: "Quotes and stubs".into()
                },
                Event::Names {
                    channel: "#thicket".into(),
                    members: vec![
                        Member {
                            nick: "grace_example".into(),
                            prefixes: "@".into()
                        },
                        Member {
                            nick: "lin_example".into(),
                            prefixes: "+".into()
                        },
                        Member {
                            nick: "ada_example".into(),
                            prefixes: "".into()
                        },
                        Member {
                            nick: "bo_example".into(),
                            prefixes: "@+".into()
                        },
                    ]
                }
            ]
        );
        let (_, ev) = feed(
            &mut s,
            &[
                ":cy_example!c@h JOIN #thicket",
                ":cy_example!c@h PART #thicket :bye",
                ":lin_example!l@h QUIT :Quit: off",
                ":grace_example!g@h KICK #thicket bo_example :calm down",
                ":grace_example!g@h MODE #thicket +ov-v+b lin_example lin_example lin_example *!*@spam",
                ":ada_example!b@h NICK :ada_away",
            ],
        );
        // join, part, quit, kick, three member modes (+b skipped), nick.
        assert_eq!(ev.len(), 8);
        assert!(matches!(
            &ev[4],
            Event::MemberMode {
                prefix: '@',
                on: true,
                ..
            }
        ));
        assert!(matches!(
            &ev[5],
            Event::MemberMode {
                prefix: '+',
                on: true,
                ..
            }
        ));
        assert!(matches!(
            &ev[6],
            Event::MemberMode {
                prefix: '+',
                on: false,
                ..
            }
        ));
        assert_eq!(s.nick(), "ada_away");
    }

    #[test]
    fn messages_actions_notices_and_ctcp() {
        let mut s = Session::new(sasl());
        let (sent, ev) = feed(
            &mut s,
            &[
                ":grace_example!g@h PRIVMSG #thicket :hello \u{3}4all",
                ":grace_example!g@h PRIVMSG #thicket :\u{1}ACTION waves\u{1}",
                ":NickServ!NickServ@services.libera.chat NOTICE ada_example :ada_example is now registered.",
                ":grace_example!g@h PRIVMSG ada_example :\u{1}VERSION\u{1}",
            ],
        );
        assert!(sent.is_empty(), "CTCP isn't answered");
        assert_eq!(ev.len(), 3);
        let Event::Message(m) = &ev[0] else { panic!() };
        assert_eq!((m.text.as_str(), m.kind), ("hello all", MsgKind::Message));
        let Event::Message(m) = &ev[1] else { panic!() };
        assert_eq!((m.text.as_str(), m.kind), ("waves", MsgKind::Action));
        let Event::Message(m) = &ev[2] else { panic!() };
        assert_eq!((m.from.as_str(), m.kind), ("NickServ", MsgKind::Notice));
    }

    #[test]
    fn commands_echo_locally_and_split_long_text() {
        let mut s = Session::new(sasl());
        let long = "word ".repeat(200);
        let out = s.command(&Command::Say {
            target: "#thicket".into(),
            text: long,
        });
        let sends: Vec<_> = out
            .iter()
            .filter_map(|o| match o {
                Output::Send(l) => Some(l.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(sends.len(), 3);
        assert!(sends.iter().all(|l| l.len() <= 400 + 20));
        let out = s.command(&Command::Me {
            target: "#thicket".into(),
            text: "waves".into(),
        });
        assert_eq!(
            out[0],
            Output::Send("PRIVMSG #thicket :\u{1}ACTION waves\u{1}".into())
        );
        assert!(
            matches!(&out[1], Output::Event(Event::Message(m)) if m.own && m.kind == MsgKind::Action)
        );
    }

    #[test]
    fn sasl_chunks_at_400() {
        let pw = "p".repeat(400);
        let lines = sasl_plain("ada", &pw);
        assert!(lines.len() >= 2);
        assert!(lines.iter().all(|l| l.len() <= "AUTHENTICATE ".len() + 400));
        // 3 + 1 + 3 + 1 + 293 = 301 raw bytes → 404 base64 chars: 400 + 4.
        let lines = sasl_plain("ada", &"p".repeat(293));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].len(), "AUTHENTICATE ".len() + 4);
        // 300 raw bytes → exactly 400: a trailing `+`.
        let lines = sasl_plain("ada", &"p".repeat(292));
        assert_eq!(lines.last().unwrap(), "AUTHENTICATE +");
    }

    #[test]
    fn split_text_keeps_characters_whole() {
        let parts = split_text(&"é".repeat(300), 401);
        assert!(parts.iter().all(|p| p.len() <= 401));
        assert_eq!(parts.concat(), "é".repeat(300));
        assert_eq!(split_text("a\nb\n\n", 10), ["a", "b"]);
    }

    #[test]
    fn debug_never_shows_a_password() {
        let a = format!("{:?}", sasl().auth);
        assert!(!a.contains("horse"));
        let c = format!(
            "{:?}",
            Command::Register {
                password: "hunter22".into(),
                email: "e@example.com".into()
            }
        );
        assert!(!c.contains("hunter22"));
    }
}
