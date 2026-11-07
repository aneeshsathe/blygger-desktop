//! Keys and pastes to the bytes a terminal sends (xterm's legacy encoding,
//! which every shell and CLI understands).

use crate::Modes;

/// A key press, in GPUI's terms (the app converts its `Keystroke`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Key {
    /// GPUI's key name: `"a"`, `"enter"`, `"up"`, `"f5"`, `"space"`, `"`"`…
    pub key: String,
    /// The text the key types with its modifiers (`"A"` with shift, `"é"`),
    /// when it types any.
    pub text: Option<String>,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// ⌘ (the platform key): the app keeps these; the terminal sends nothing.
    pub cmd: bool,
}

impl Key {
    /// A key with no modifiers (tests).
    pub fn named(key: &str) -> Key {
        Key {
            key: key.into(),
            ..Default::default()
        }
    }

    /// A character typed (tests).
    pub fn typed(text: &str) -> Key {
        Key {
            key: text.to_lowercase(),
            text: Some(text.into()),
            shift: text.chars().any(char::is_uppercase),
            ..Default::default()
        }
    }

    pub fn ctrl(key: &str) -> Key {
        Key {
            key: key.into(),
            ctrl: true,
            ..Default::default()
        }
    }
}

/// xterm's modifier parameter: 1 + shift + 2·alt + 4·ctrl.
fn modifier_param(k: &Key) -> u8 {
    1 + k.shift as u8 + 2 * k.alt as u8 + 4 * k.ctrl as u8
}

fn esc(s: &str) -> Vec<u8> {
    let mut v = vec![0x1b];
    v.extend_from_slice(s.as_bytes());
    v
}

/// The bytes `key` sends, or `None` for a key the terminal leaves alone.
pub fn encode(key: &Key, modes: Modes) -> Option<Vec<u8>> {
    if key.cmd {
        return None;
    }
    let m = modifier_param(key);
    let mods = m > 1;
    let alt_prefix = |mut bytes: Vec<u8>| {
        if key.alt {
            bytes.insert(0, 0x1b);
        }
        bytes
    };
    // Cursor keys and friends: `ESC [ 1 ; m X` with modifiers, else
    // `ESC [ X` (or `ESC O X` in application cursor mode).
    let cursor = |c: char| {
        if mods {
            esc(&format!("[1;{m}{c}"))
        } else if modes.app_cursor {
            esc(&format!("O{c}"))
        } else {
            esc(&format!("[{c}"))
        }
    };
    let tilde = |n: u8| {
        if mods {
            esc(&format!("[{n};{m}~"))
        } else {
            esc(&format!("[{n}~"))
        }
    };
    let ss3 = |c: char| {
        if mods {
            esc(&format!("[1;{m}{c}"))
        } else {
            esc(&format!("O{c}"))
        }
    };
    let bytes = match key.key.as_str() {
        "enter" => alt_prefix(vec![b'\r']),
        "tab" if key.shift => esc("[Z"),
        "tab" => alt_prefix(vec![b'\t']),
        "backspace" if key.ctrl => alt_prefix(vec![0x08]),
        "backspace" => alt_prefix(vec![0x7f]),
        "escape" => alt_prefix(vec![0x1b]),
        "space" if key.ctrl => alt_prefix(vec![0]),
        "space" => alt_prefix(vec![b' ']),
        "up" => cursor('A'),
        "down" => cursor('B'),
        "right" => cursor('C'),
        "left" => cursor('D'),
        "home" => cursor('H'),
        "end" => cursor('F'),
        "insert" => tilde(2),
        "delete" => tilde(3),
        "pageup" => tilde(5),
        "pagedown" => tilde(6),
        "f1" => ss3('P'),
        "f2" => ss3('Q'),
        "f3" => ss3('R'),
        "f4" => ss3('S'),
        "f5" => tilde(15),
        "f6" => tilde(17),
        "f7" => tilde(18),
        "f8" => tilde(19),
        "f9" => tilde(20),
        "f10" => tilde(21),
        "f11" => tilde(23),
        "f12" => tilde(24),
        k if key.ctrl => {
            let mut chars = k.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return None;
            };
            alt_prefix(vec![ctrl_byte(c)?])
        }
        k => {
            // What the key types; ⌥ with an unchanged key is Meta (ESC
            // prefix), ⌥ that composed a character (⌥e é) types it.
            let text = key.text.clone().or_else(|| {
                let mut chars = k.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Some(c.to_string()),
                    _ => None,
                }
            })?;
            if key.alt && (text == k || key.text.is_none()) {
                esc(&text)
            } else {
                text.into_bytes()
            }
        }
    };
    Some(bytes)
}

/// The control byte for ⌃`c`, as xterm sends it.
fn ctrl_byte(c: char) -> Option<u8> {
    let c = c.to_ascii_lowercase();
    Some(match c {
        'a'..='z' => c as u8 - b'a' + 1,
        '@' | '2' | ' ' | '`' => 0,
        '[' | '3' => 0x1b,
        '\\' | '4' => 0x1c,
        ']' | '5' => 0x1d,
        '^' | '6' => 0x1e,
        '_' | '-' | '7' => 0x1f,
        '?' | '8' => 0x7f,
        _ => return None,
    })
}

/// A paste: newlines become carriage returns (what ⏎ sends) and, when the
/// program asked for bracketed paste, the text is wrapped so it can't run
/// anything by itself. A paste can't close the bracket early: any end
/// marker inside is removed, and so is every other escape character.
pub fn encode_paste(text: &str, modes: Modes) -> Vec<u8> {
    let clean: String = text
        .replace("\x1b[201~", "")
        .replace("\r\n", "\r")
        .replace('\n', "\r")
        .chars()
        .filter(|&c| c != '\x1b')
        .collect();
    if modes.bracketed_paste {
        let mut v = b"\x1b[200~".to_vec();
        v.extend_from_slice(clean.as_bytes());
        v.extend_from_slice(b"\x1b[201~");
        v
    } else {
        clean.into_bytes()
    }
}

/// `text` as a Markdown quote (`> ` on every line), for "send selection to
/// the terminal".
pub fn quote(text: &str) -> String {
    text.trim_end_matches(['\n', '\r'])
        .lines()
        .map(|l| {
            if l.is_empty() {
                ">".to_string()
            } else {
                format!("> {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(k: Key) -> Vec<u8> {
        encode(&k, Modes::default()).unwrap()
    }

    #[test]
    fn plain_keys() {
        assert_eq!(enc(Key::typed("a")), b"a");
        assert_eq!(enc(Key::typed("A")), b"A");
        assert_eq!(enc(Key::named("enter")), b"\r");
        assert_eq!(enc(Key::named("backspace")), b"\x7f");
        assert_eq!(enc(Key::named("tab")), b"\t");
        assert_eq!(enc(Key::named("escape")), b"\x1b");
        assert_eq!(enc(Key::named("space")), b" ");
        assert_eq!(
            enc(Key {
                shift: true,
                ..Key::named("tab")
            }),
            b"\x1b[Z"
        );
    }

    #[test]
    fn control_keys() {
        assert_eq!(enc(Key::ctrl("c")), [3]);
        assert_eq!(enc(Key::ctrl("d")), [4]);
        assert_eq!(enc(Key::ctrl("[")), [0x1b]);
        assert_eq!(enc(Key::ctrl("space")), [0]);
        assert_eq!(
            enc(Key {
                alt: true,
                ..Key::ctrl("x")
            }),
            [0x1b, 0x18]
        );
    }

    #[test]
    fn cursor_keys_follow_the_mode() {
        assert_eq!(enc(Key::named("up")), b"\x1b[A");
        let app = Modes {
            app_cursor: true,
            ..Default::default()
        };
        assert_eq!(encode(&Key::named("up"), app).unwrap(), b"\x1bOA");
        assert_eq!(
            enc(Key {
                ctrl: true,
                ..Key::named("left")
            }),
            b"\x1b[1;5D"
        );
        assert_eq!(enc(Key::named("pageup")), b"\x1b[5~");
        assert_eq!(enc(Key::named("f1")), b"\x1bOP");
        assert_eq!(enc(Key::named("f5")), b"\x1b[15~");
    }

    #[test]
    fn option_is_meta_unless_it_composed_a_character() {
        let meta = Key {
            key: "b".into(),
            text: Some("b".into()),
            alt: true,
            ..Default::default()
        };
        assert_eq!(enc(meta), b"\x1bb");
        let composed = Key {
            key: "e".into(),
            text: Some("é".into()),
            alt: true,
            ..Default::default()
        };
        assert_eq!(enc(composed), "é".as_bytes());
    }

    #[test]
    fn command_keys_stay_with_the_app() {
        let k = Key {
            cmd: true,
            ..Key::typed("c")
        };
        assert_eq!(encode(&k, Modes::default()), None);
    }

    #[test]
    fn pastes() {
        let plain = Modes::default();
        assert_eq!(encode_paste("a\nb", plain), b"a\rb");
        let bracketed = Modes {
            bracketed_paste: true,
            ..Default::default()
        };
        assert_eq!(
            encode_paste("ls\x1b[201~; rm x\n", bracketed),
            b"\x1b[200~ls; rm x\r\x1b[201~"
        );
    }

    #[test]
    fn quoting() {
        assert_eq!(quote("one\n\ntwo\n"), "> one\n>\n> two");
    }
}
