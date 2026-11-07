//! --- chat --- The chat tab's decisions, with no GPUI: what the compose box
//! sends, setup's checks, the tab's marks, the Keychain account, and the
//! words for the connection's state.

use std::time::Duration;

/// The line in the empty state, the setup screen and the docs.
pub const EXPERIMENTAL: &str = "Experimental: this may be removed.";

/// What the compose box's text means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compose {
    Nothing,
    Say(String),
    Me(String),
    /// A `/command` the tab doesn't do.
    Unknown(String),
}

/// `/me waves` is an action, `//text` says `/text`, any other `/word` is
/// unknown, and everything else is said as it is.
pub fn compose(text: &str) -> Compose {
    let t = text.trim();
    if t.is_empty() {
        return Compose::Nothing;
    }
    if let Some(rest) = t.strip_prefix("//") {
        return Compose::Say(format!("/{rest}"));
    }
    if let Some(rest) = t.strip_prefix('/') {
        let (cmd, arg) = rest.split_once(' ').unwrap_or((rest, ""));
        if cmd.eq_ignore_ascii_case("me") {
            let arg = arg.trim();
            return if arg.is_empty() {
                Compose::Nothing
            } else {
                Compose::Me(arg.to_string())
            };
        }
        return Compose::Unknown(format!("/{cmd}"));
    }
    Compose::Say(t.to_string())
}

/// The Keychain account a Libera password is kept under, beside the blyg
/// sign-ins in the same service: `irc:libera:<nick>`.
pub fn keychain_account(server: &str, nick: &str) -> String {
    format!("irc:{}:{}", network(server), blyg_irc::fold(nick))
}

/// A short name for the network: `libera` for Libera.Chat, else the host.
pub fn network(server: &str) -> String {
    let host = server
        .rsplit_once(':')
        .map(|(h, _)| h)
        .unwrap_or(server)
        .trim()
        .to_ascii_lowercase();
    if host == "libera.chat" || host.ends_with(".libera.chat") {
        "libera".into()
    } else {
        host
    }
}

/// Setup's check of a nick: Libera's rules (at most 16 characters).
pub fn check_nick(nick: &str) -> Result<String, &'static str> {
    let n = nick.trim();
    if n.is_empty() {
        return Err("Pick a nick.");
    }
    if !blyg_irc::valid_nick(n, blyg_irc::NICK_MAX) {
        return Err(
            "A nick is up to 16 letters, digits or _-[]{}|^`\\, and doesn't start with a \
             digit or a dash.",
        );
    }
    Ok(n.to_string())
}

/// A password NickServ can take: no spaces (REGISTER's words are split on
/// them), and not too short.
pub fn check_password(pw: &str) -> Result<(), &'static str> {
    if pw.is_empty() {
        return Err("Enter a password.");
    }
    if pw.chars().any(char::is_whitespace) {
        return Err("Use a password without spaces: NickServ reads a space as the end of it.");
    }
    if pw.chars().count() < 8 {
        return Err("Use at least 8 characters.");
    }
    Ok(())
}

pub fn check_email(email: &str) -> Result<String, &'static str> {
    let e = email.trim();
    let ok = e
        .split_once('@')
        .is_some_and(|(u, d)| !u.is_empty() && d.contains('.') && !d.ends_with('.'))
        && !e.contains(char::is_whitespace);
    if ok {
        Ok(e.to_string())
    } else {
        Err("Enter a real email address: Libera.Chat sends the code there.")
    }
}

/// The code from the email: the last word, so pasting the whole
/// `/msg NickServ VERIFY REGISTER ada_example abcdefgh` line works too.
pub fn verify_code(text: &str) -> Option<String> {
    text.split_whitespace()
        .last()
        .filter(|w| w.chars().all(|c| c.is_ascii_alphanumeric()))
        .map(str::to_string)
}

/// Whether NickServ's notice says the account was verified.
pub fn says_verified(notice: &str) -> bool {
    let n = notice.to_ascii_lowercase();
    n.contains("has now been verified") || n.contains("has been verified")
}

/// What the tab shows beside its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mark {
    None,
    /// Unread messages: a dot and the count.
    Unread(usize),
    /// Someone mentioned you: the stronger mark, with how many times.
    Mention(usize),
}

pub fn mark(unread: usize, mentions: usize) -> Mark {
    if mentions > 0 {
        Mark::Mention(mentions)
    } else if unread > 0 {
        Mark::Unread(unread)
    } else {
        Mark::None
    }
}

/// A count for a small badge: `9+` past nine.
pub fn count(n: usize) -> String {
    if n > 9 { "9+".into() } else { n.to_string() }
}

/// The connection, as the tab's header says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conn {
    /// Not set up, or turned off: nothing connects.
    Off,
    Connecting,
    /// Connected, signing in (before the welcome).
    SigningIn,
    Connected,
    Retrying(Duration),
    /// Stopped for good (the password was refused…).
    Failed(String),
}

pub fn conn_text(c: &Conn) -> String {
    match c {
        Conn::Off => "not connected".into(),
        Conn::Connecting => "connecting…".into(),
        Conn::SigningIn => "signing in…".into(),
        Conn::Connected => "connected".into(),
        Conn::Retrying(d) => {
            let s = d.as_secs().max(1);
            format!("offline · retrying in {s} s")
        }
        Conn::Failed(why) => why.clone(),
    }
}

/// `14:05`, in local time, for a line's timestamp.
pub fn clock(at: u64) -> String {
    use chrono::TimeZone as _;
    chrono::Local
        .timestamp_opt(at as i64, 0)
        .single()
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_default()
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A notification's body: `grace_example: ada_example, look`, cut short.
pub fn notification_body(nick: &str, text: &str) -> String {
    let mut s = format!("{nick}: {text}");
    if s.chars().count() > 160 {
        s = s.chars().take(159).collect::<String>() + "…";
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_commands() {
        assert_eq!(compose("  "), Compose::Nothing);
        assert_eq!(compose("hello"), Compose::Say("hello".into()));
        assert_eq!(compose("/me waves"), Compose::Me("waves".into()));
        assert_eq!(compose("/ME  waves "), Compose::Me("waves".into()));
        assert_eq!(compose("/me"), Compose::Nothing);
        assert_eq!(compose("//shrug"), Compose::Say("/shrug".into()));
        assert_eq!(compose("/join #other"), Compose::Unknown("/join".into()));
    }

    #[test]
    fn keychain_accounts() {
        assert_eq!(
            keychain_account("irc.libera.chat:6697", "Ada_Example"),
            "irc:libera:ada_example"
        );
        assert_eq!(
            keychain_account("127.0.0.1:6667", "ada_example"),
            "irc:127.0.0.1:ada_example"
        );
        // The account doesn't parse as a URL with a host, so the token
        // store keeps it as it is (`token_account`).
        assert_eq!(
            blyg_core::config::token_account("irc:libera:ada_example"),
            "irc:libera:ada_example"
        );
    }

    #[test]
    fn setup_checks() {
        assert_eq!(check_nick(" ada_example "), Ok("ada_example".into()));
        assert!(check_nick("9lives").is_err());
        assert!(check_nick("").is_err());
        assert!(check_password("correct horse").is_err());
        assert!(check_password("short").is_err());
        assert!(check_password("correct-horse-7").is_ok());
        assert!(check_email("ada@example.com").is_ok());
        assert!(check_email("ada@example").is_err());
        assert!(check_email("ada example@x.org").is_err());
        assert_eq!(verify_code(" abcdefgh "), Some("abcdefgh".into()));
        assert_eq!(
            verify_code("/msg NickServ VERIFY REGISTER ada_example abcdefgh"),
            Some("abcdefgh".into())
        );
        assert_eq!(verify_code(""), None);
        assert!(says_verified("ada_example has now been verified."));
        assert!(!says_verified(
            "Verification failed. Invalid key for ada_example."
        ));
    }

    #[test]
    fn marks() {
        assert_eq!(mark(0, 0), Mark::None);
        assert_eq!(mark(3, 0), Mark::Unread(3));
        assert_eq!(mark(3, 1), Mark::Mention(1));
        assert_eq!(count(12), "9+");
        assert_eq!(count(4), "4");
    }

    #[test]
    fn words_for_the_connection() {
        assert_eq!(conn_text(&Conn::Connected), "connected");
        assert_eq!(
            conn_text(&Conn::Retrying(Duration::from_millis(400))),
            "offline · retrying in 1 s"
        );
        assert_eq!(
            notification_body("grace_example", &"x".repeat(300))
                .chars()
                .count(),
            160
        );
    }
}
