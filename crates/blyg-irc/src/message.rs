//! IRC messages (RFC 1459 / 2812 framing with IRCv3 message tags): parse a
//! line from the server, and build lines to send.
//!
//! `@tags :prefix COMMAND param param :trailing param`

use std::fmt;

/// Where a message came from: `nick!user@host`, or a server name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefix {
    /// The nick, or the whole prefix for a server (`irc.example.org`).
    pub nick: String,
    pub user: Option<String>,
    pub host: Option<String>,
}

impl Prefix {
    pub fn parse(s: &str) -> Prefix {
        let (rest, host) = match s.split_once('@') {
            Some((r, h)) => (r, Some(h.to_string())),
            None => (s, None),
        };
        let (nick, user) = match rest.split_once('!') {
            Some((n, u)) => (n.to_string(), Some(u.to_string())),
            None => (rest.to_string(), None),
        };
        Prefix { nick, user, host }
    }
}

/// One IRC message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// IRCv3 tags, unescaped (`None` for a tag with no value).
    pub tags: Vec<(String, Option<String>)>,
    pub prefix: Option<Prefix>,
    /// Upper-cased (`PRIVMSG`), or a three-digit numeric (`001`).
    pub command: String,
    pub params: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    NoCommand,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "empty line"),
            ParseError::NoCommand => write!(f, "no command"),
        }
    }
}

impl Message {
    /// A message with no tags or prefix (what a client sends).
    pub fn new(command: &str, params: &[&str]) -> Message {
        Message {
            tags: Vec::new(),
            prefix: None,
            command: command.to_string(),
            params: params.iter().map(|p| p.to_string()).collect(),
        }
    }

    /// Parse one line (with or without its CR LF).
    pub fn parse(line: &str) -> Result<Message, ParseError> {
        let mut rest = line.trim_end_matches(['\r', '\n']).trim_start_matches(' ');
        if rest.is_empty() {
            return Err(ParseError::Empty);
        }
        let mut tags = Vec::new();
        if let Some(r) = rest.strip_prefix('@') {
            let (t, r) = r.split_once(' ').unwrap_or((r, ""));
            for tag in t.split(';').filter(|t| !t.is_empty()) {
                match tag.split_once('=') {
                    Some((k, v)) => tags.push((k.to_string(), Some(unescape_tag(v)))),
                    None => tags.push((tag.to_string(), None)),
                }
            }
            rest = r.trim_start_matches(' ');
        }
        let mut prefix = None;
        if let Some(r) = rest.strip_prefix(':') {
            let (p, r) = r.split_once(' ').unwrap_or((r, ""));
            prefix = Some(Prefix::parse(p));
            rest = r.trim_start_matches(' ');
        }
        let (command, mut rest) = match rest.split_once(' ') {
            Some((c, r)) => (c, r),
            None => (rest, ""),
        };
        if command.is_empty() {
            return Err(ParseError::NoCommand);
        }
        let mut params = Vec::new();
        loop {
            rest = rest.trim_start_matches(' ');
            if rest.is_empty() {
                break;
            }
            if let Some(trailing) = rest.strip_prefix(':') {
                params.push(trailing.to_string());
                break;
            }
            match rest.split_once(' ') {
                Some((p, r)) => {
                    params.push(p.to_string());
                    rest = r;
                }
                None => {
                    params.push(rest.to_string());
                    break;
                }
            }
        }
        Ok(Message {
            tags,
            prefix,
            command: command.to_ascii_uppercase(),
            params,
        })
    }

    /// The sender's nick (or server name), if there's a prefix.
    pub fn source(&self) -> Option<&str> {
        self.prefix.as_ref().map(|p| p.nick.as_str())
    }

    pub fn param(&self, i: usize) -> Option<&str> {
        self.params.get(i).map(String::as_str)
    }

    /// The last parameter (usually the human-readable text).
    pub fn trailing(&self) -> Option<&str> {
        self.params.last().map(String::as_str)
    }

    /// The tag's value, if the message carries it.
    pub fn tag(&self, key: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| v.as_deref())
    }

    /// The wire form, without CR LF. CR, LF and NUL in any parameter are
    /// replaced with spaces so a message can never smuggle a second line.
    /// Tags are never sent (Burrow requests no client tags).
    pub fn to_line(&self) -> String {
        let mut out = String::new();
        if let Some(p) = &self.prefix {
            out.push(':');
            out.push_str(&p.nick);
            if let Some(u) = &p.user {
                out.push('!');
                out.push_str(u);
            }
            if let Some(h) = &p.host {
                out.push('@');
                out.push_str(h);
            }
            out.push(' ');
        }
        out.push_str(&clean(&self.command));
        let n = self.params.len();
        for (i, p) in self.params.iter().enumerate() {
            let p = clean(p);
            out.push(' ');
            let last = i + 1 == n;
            if last && (p.is_empty() || p.contains(' ') || p.starts_with(':')) {
                out.push(':');
            }
            out.push_str(&p);
        }
        out
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_line())
    }
}

fn clean(s: &str) -> String {
    s.chars()
        .map(|c| {
            if matches!(c, '\r' | '\n' | '\0') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

fn unescape_tag(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some(':') => out.push(';'),
            Some('s') => out.push(' '),
            Some('\\') => out.push('\\'),
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

/// A CTCP-wrapped text (`\x01ACTION waves\x01`): its command and argument.
pub fn ctcp(text: &str) -> Option<(&str, &str)> {
    let inner = text.strip_prefix('\u{1}')?;
    let inner = inner.strip_suffix('\u{1}').unwrap_or(inner);
    Some(match inner.split_once(' ') {
        Some((c, a)) => (c, a),
        None => (inner, ""),
    })
}

/// Formatting codes (bold, colour, italics…) removed, for display.
pub fn strip_formatting(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{2}' | '\u{1d}' | '\u{1f}' | '\u{1e}' | '\u{11}' | '\u{16}' | '\u{f}' => {}
            '\u{3}' => {
                // ^C[fg[,bg]]: up to two digits each.
                let mut fg = false;
                for _ in 0..2 {
                    if chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                        chars.next();
                        fg = true;
                    }
                }
                if fg && chars.peek() == Some(&',') {
                    let mut look = chars.clone();
                    look.next();
                    if look.peek().is_some_and(|c| c.is_ascii_digit()) {
                        chars.next();
                        for _ in 0..2 {
                            if chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                                chars.next();
                            }
                        }
                    }
                }
            }
            '\u{4}' => {
                // ^D + six hex digits (hex colour).
                for _ in 0..6 {
                    if chars.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                        chars.next();
                    }
                }
            }
            c if (c as u32) < 0x20 && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// IRC's case-insensitive form of a nick or channel (rfc1459 casemapping:
/// `[]\~` are the upper case of `{}|^`).
pub fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '[' => '{',
            ']' => '}',
            '\\' => '|',
            '~' => '^',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

/// Whether `nick` is a nick a server would accept: 1–`max` characters,
/// letters, digits and `[]\`_^{|}-`, not starting with a digit or `-`.
pub fn valid_nick(nick: &str, max: usize) -> bool {
    let n = nick.chars().count();
    if n == 0 || n > max {
        return false;
    }
    let ok = |c: char| c.is_ascii_alphanumeric() || "[]\\`_^{|}-".contains(c);
    let first = nick.chars().next().unwrap_or('0');
    nick.chars().all(ok) && !first.is_ascii_digit() && first != '-'
}

/// Whether `text` mentions `nick` as a word (case-insensitive): "ada_example:
/// hi" and "thanks ada_example!" do, "ada_examples" doesn't.
pub fn mentions(text: &str, nick: &str) -> bool {
    if nick.is_empty() {
        return false;
    }
    let text = fold(text);
    let nick = fold(nick);
    let word = |c: char| c.is_alphanumeric() || "[]\\`_^{|}-".contains(c);
    let mut from = 0;
    while let Some(i) = text[from..].find(&nick) {
        let start = from + i;
        let end = start + nick.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        if !before.is_some_and(word) && !after.is_some_and(word) {
            return true;
        }
        from = start + nick.len().max(1);
        while !text.is_char_boundary(from) {
            from += 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_privmsg_with_prefix_and_trailing() {
        let m =
            Message::parse(":ada_example!~ada@user/ada PRIVMSG #thicket :hello there\r\n").unwrap();
        assert_eq!(m.command, "PRIVMSG");
        assert_eq!(m.source(), Some("ada_example"));
        let p = m.prefix.as_ref().unwrap();
        assert_eq!(p.user.as_deref(), Some("~ada"));
        assert_eq!(p.host.as_deref(), Some("user/ada"));
        assert_eq!(m.params, ["#thicket", "hello there"]);
    }

    #[test]
    fn parses_tags_with_escapes() {
        let m = Message::parse(
            "@time=2026-11-06T10:00:00.000Z;msgid=abc;flag;note=a\\sb\\:c :srv NOTICE * :hi",
        )
        .unwrap();
        assert_eq!(m.tag("time"), Some("2026-11-06T10:00:00.000Z"));
        assert_eq!(m.tag("note"), Some("a b;c"));
        assert_eq!(m.tag("flag"), None);
        assert!(m.tags.iter().any(|(k, v)| k == "flag" && v.is_none()));
        assert_eq!(m.source(), Some("srv"));
    }

    #[test]
    fn parses_numerics_middle_params_and_empty_trailing() {
        let m = Message::parse(":irc.example.org 353 ada = #thicket :@op +voice plain").unwrap();
        assert_eq!(m.command, "353");
        assert_eq!(m.params, ["ada", "=", "#thicket", "@op +voice plain"]);
        let m = Message::parse("PING :").unwrap();
        assert_eq!(m.params, [""]);
        let m = Message::parse("ping token").unwrap();
        assert_eq!(m.command, "PING");
        assert_eq!(m.params, ["token"]);
        let m = Message::parse(":srv CAP * LS * :sasl multi-prefix").unwrap();
        assert_eq!(m.params, ["*", "LS", "*", "sasl multi-prefix"]);
    }

    #[test]
    fn extra_spaces_and_bad_lines() {
        let m = Message::parse(":a  JOIN   #c").unwrap();
        assert_eq!(m.params, ["#c"]);
        assert_eq!(Message::parse("\r\n"), Err(ParseError::Empty));
        assert_eq!(Message::parse(":onlyprefix"), Err(ParseError::NoCommand));
        let m = Message::parse(":server.only 001 me :Welcome").unwrap();
        assert_eq!(m.prefix.unwrap().user, None);
    }

    #[test]
    fn serialises_and_round_trips() {
        let m = Message::new("PRIVMSG", &["#thicket", "hi there"]);
        assert_eq!(m.to_line(), "PRIVMSG #thicket :hi there");
        assert_eq!(Message::new("NICK", &["ada"]).to_line(), "NICK ada");
        assert_eq!(
            Message::new("CAP", &["REQ", "sasl"]).to_line(),
            "CAP REQ sasl"
        );
        assert_eq!(Message::new("PONG", &[""]).to_line(), "PONG :");
        assert_eq!(
            Message::new("PRIVMSG", &["#c", ":)"]).to_line(),
            "PRIVMSG #c ::)"
        );
        let back = Message::parse(&Message::new("PRIVMSG", &["#c", "a :b"]).to_line()).unwrap();
        assert_eq!(back.params, ["#c", "a :b"]);
    }

    #[test]
    fn never_sends_a_second_line() {
        let m = Message::new("PRIVMSG", &["#thicket", "hi\r\nQUIT :bye"]);
        assert_eq!(m.to_line(), "PRIVMSG #thicket :hi  QUIT :bye");
        assert!(!m.to_line().contains('\n'));
    }

    #[test]
    fn ctcp_actions() {
        assert_eq!(ctcp("\u{1}ACTION waves\u{1}"), Some(("ACTION", "waves")));
        assert_eq!(ctcp("\u{1}VERSION\u{1}"), Some(("VERSION", "")));
        assert_eq!(
            ctcp("\u{1}ACTION unterminated"),
            Some(("ACTION", "unterminated"))
        );
        assert_eq!(ctcp("plain"), None);
    }

    #[test]
    fn strips_formatting() {
        assert_eq!(strip_formatting("\u{2}bold\u{2} text"), "bold text");
        assert_eq!(
            strip_formatting("\u{3}4,12red\u{3} and \u{3}3green"),
            "red and green"
        );
        // A background needs a foreground first.
        assert_eq!(strip_formatting("\u{3},5 comma stays"), ",5 comma stays");
        assert_eq!(strip_formatting("\u{4}ff00aahex"), "hex");
    }

    #[test]
    fn casefolds_rfc1459() {
        assert_eq!(fold("Ada[Example]"), "ada{example}");
        assert_eq!(fold("A\\B~"), "a|b^");
    }

    #[test]
    fn nick_rules() {
        assert!(valid_nick("ada_example", 16));
        assert!(valid_nick("[ada]", 16));
        assert!(!valid_nick("1ada", 16));
        assert!(!valid_nick("-ada", 16));
        assert!(!valid_nick("ada example", 16));
        assert!(!valid_nick("", 16));
        assert!(!valid_nick("a_very_long_nickname", 16));
    }

    #[test]
    fn mention_is_a_whole_word() {
        assert!(mentions("ada_example: hi", "ada_example"));
        assert!(mentions("thanks, Ada_Example!", "ada_example"));
        assert!(mentions("ping ada", "ADA"));
        assert!(!mentions("ada_examples rock", "ada_example"));
        assert!(!mentions("grenada", "ada"));
        assert!(mentions("grenada and ada", "ada"));
        assert!(!mentions("anything", ""));
        assert!(mentions("é ada é", "ada"));
    }
}
