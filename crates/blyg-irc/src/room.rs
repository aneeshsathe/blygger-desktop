//! One channel as the UI shows it: the members (with their prefixes) and a
//! bounded log of what was said. `apply` folds the session's events in.

use std::collections::{BTreeMap, VecDeque};

use crate::message::{fold, mentions};
use crate::session::{Event, Member, MsgKind};

/// Prefix symbols from highest to lowest, for ordering and "is an op".
const RANK: &str = "~&@%+";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Message,
    Action,
    Notice,
    /// A join, part, quit, kick or nick change: shown quietly.
    Membership,
    Topic,
    /// Burrow's own note (connected, reconnecting…).
    Status,
}

/// One line of the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Seconds since the Unix epoch.
    pub at: u64,
    pub kind: LineKind,
    /// Who; empty for status lines.
    pub nick: String,
    pub text: String,
    /// Our own message.
    pub own: bool,
    /// It names our nick (or was sent to us privately).
    pub mention: bool,
    /// Sent privately to us rather than to the channel.
    pub private: bool,
}

impl Line {
    /// Someone else said something (what counts as unread).
    pub fn is_chat(&self) -> bool {
        !self.own && matches!(self.kind, LineKind::Message | LineKind::Action)
            || (self.private && !self.own)
    }
}

/// What `apply` did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Applied {
    /// A line was added to the log.
    pub line: bool,
    /// It's someone else's message (counts as unread).
    pub chat: bool,
    /// It mentions us.
    pub mention: bool,
}

#[derive(Debug, Clone)]
pub struct Room {
    pub name: String,
    pub topic: Option<String>,
    /// Our nick, for mentions.
    pub me: String,
    pub joined: bool,
    members: BTreeMap<String, Member>,
    log: VecDeque<Line>,
    cap: usize,
}

impl Room {
    pub fn new(name: &str, me: &str, cap: usize) -> Room {
        Room {
            name: name.to_string(),
            topic: None,
            me: me.to_string(),
            joined: false,
            members: BTreeMap::new(),
            log: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    pub fn log(&self) -> impl ExactSizeIterator<Item = &Line> + DoubleEndedIterator {
        self.log.iter()
    }

    pub fn log_len(&self) -> usize {
        self.log.len()
    }

    /// Members: ops, then half-ops, then voiced, then everyone, each
    /// alphabetically (case-insensitive).
    pub fn members(&self) -> Vec<&Member> {
        let mut v: Vec<&Member> = self.members.values().collect();
        v.sort_by_key(|m| (rank(&m.prefixes), fold(&m.nick)));
        v
    }

    pub fn member(&self, nick: &str) -> Option<&Member> {
        self.members.get(&fold(nick))
    }

    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// A note of our own in the log (connected, reconnecting…).
    pub fn status(&mut self, text: &str, now: u64) {
        self.push(Line {
            at: now,
            kind: LineKind::Status,
            nick: String::new(),
            text: text.to_string(),
            own: false,
            mention: false,
            private: false,
        });
    }

    pub fn clear_members(&mut self) {
        self.members.clear();
        self.joined = false;
    }

    /// Fold one event in (events for other channels are ignored, except
    /// private messages to us).
    pub fn apply(&mut self, ev: &Event, now: u64) -> Applied {
        let here = |c: &str| fold(c) == fold(&self.name);
        let mut a = Applied::default();
        let quiet = |nick: &str, text: String| Line {
            at: now,
            kind: LineKind::Membership,
            nick: nick.to_string(),
            text,
            own: false,
            mention: false,
            private: false,
        };
        match ev {
            Event::Registered { nick } => self.me = nick.clone(),
            Event::Joined { channel } if here(channel) => {
                self.joined = true;
                self.members.clear();
            }
            Event::Names { channel, members } if here(channel) => {
                self.members = members
                    .iter()
                    .map(|m| {
                        let mut m = m.clone();
                        m.prefixes = sorted(&m.prefixes);
                        (fold(&m.nick), m)
                    })
                    .collect();
            }
            Event::Join { channel, nick } if here(channel) => {
                self.members.insert(
                    fold(nick),
                    Member {
                        nick: nick.clone(),
                        prefixes: String::new(),
                    },
                );
                a.line = self.push(quiet(nick, "joined".into()));
            }
            Event::Part {
                channel,
                nick,
                reason,
            } if here(channel) => {
                if fold(nick) == fold(&self.me) {
                    self.clear_members();
                } else {
                    self.members.remove(&fold(nick));
                }
                a.line = self.push(quiet(nick, with_reason("left", reason)));
            }
            Event::Quit { nick, reason } => {
                if self.members.remove(&fold(nick)).is_some() {
                    a.line = self.push(quiet(nick, with_reason("quit", reason)));
                }
            }
            Event::Kick {
                channel,
                nick,
                by,
                reason,
            } if here(channel) => {
                if fold(nick) == fold(&self.me) {
                    self.clear_members();
                } else {
                    self.members.remove(&fold(nick));
                }
                a.line = self.push(quiet(
                    nick,
                    with_reason(&format!("was removed by {by}"), reason),
                ));
            }
            Event::Nick { old, new } => {
                if fold(old) == fold(&self.me) {
                    self.me = new.clone();
                }
                if let Some(mut m) = self.members.remove(&fold(old)) {
                    m.nick = new.clone();
                    self.members.insert(fold(new), m);
                    a.line = self.push(quiet(old, format!("is now {new}")));
                }
            }
            Event::MemberMode {
                channel,
                nick,
                prefix,
                on,
            } if here(channel) => {
                if let Some(m) = self.members.get_mut(&fold(nick)) {
                    let mut p: String = m.prefixes.chars().filter(|c| c != prefix).collect();
                    if *on {
                        p.push(*prefix);
                    }
                    m.prefixes = sorted(&p);
                }
            }
            Event::Topic { channel, topic } if here(channel) => {
                let changed = self.topic.is_some();
                self.topic = Some(topic.clone()).filter(|t| !t.is_empty());
                if changed {
                    a.line = self.push(Line {
                        at: now,
                        kind: LineKind::Topic,
                        nick: String::new(),
                        text: topic.clone(),
                        own: false,
                        mention: false,
                        private: false,
                    });
                }
            }
            Event::Message(m) => {
                let private = fold(&m.target) == fold(&self.me) && !m.own;
                if !here(&m.target) && !private {
                    return a;
                }
                // Services and the server talk to us privately; those
                // notices aren't chat (the setup screen shows NickServ's).
                let service = m.kind == MsgKind::Notice
                    && (m.from.contains('.') || fold(&m.from).ends_with("serv"));
                if private && service {
                    return a;
                }
                let mention = !m.own && (private || mentions(&m.text, &self.me));
                let line = Line {
                    at: now,
                    kind: match m.kind {
                        MsgKind::Message => LineKind::Message,
                        MsgKind::Action => LineKind::Action,
                        MsgKind::Notice => LineKind::Notice,
                    },
                    nick: m.from.clone(),
                    text: m.text.clone(),
                    own: m.own,
                    mention,
                    private,
                };
                a.chat = line.is_chat();
                a.mention = mention;
                a.line = self.push(line);
            }
            _ => {}
        }
        a
    }

    fn push(&mut self, line: Line) -> bool {
        if self.log.len() == self.cap {
            self.log.pop_front();
        }
        self.log.push_back(line);
        true
    }
}

/// Lower is higher: `~` 0 … `+` 4, none 5.
fn rank(prefixes: &str) -> usize {
    prefixes
        .chars()
        .filter_map(|c| RANK.find(c))
        .min()
        .unwrap_or(RANK.len())
}

fn sorted(prefixes: &str) -> String {
    let mut v: Vec<char> = prefixes.chars().collect();
    v.sort_by_key(|c| RANK.find(*c).unwrap_or(RANK.len()));
    v.dedup();
    v.into_iter().collect()
}

/// Whether a member's prefixes make them an operator (`~`, `&`, `@` or `%`).
pub fn is_op(prefixes: &str) -> bool {
    prefixes.chars().any(|c| "~&@%".contains(c))
}

fn with_reason(what: &str, reason: &str) -> String {
    if reason.trim().is_empty() {
        what.to_string()
    } else {
        format!("{what} ({})", reason.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::ChatMessage;

    fn msg(target: &str, from: &str, text: &str) -> Event {
        Event::Message(ChatMessage {
            target: target.into(),
            from: from.into(),
            text: text.into(),
            kind: MsgKind::Message,
            own: false,
        })
    }

    fn member(nick: &str, prefixes: &str) -> Member {
        Member {
            nick: nick.into(),
            prefixes: prefixes.into(),
        }
    }

    fn joined() -> Room {
        let mut r = Room::new("#thicket", "ada_example", 5);
        r.apply(
            &Event::Joined {
                channel: "#thicket".into(),
            },
            1,
        );
        r.apply(
            &Event::Names {
                channel: "#Thicket".into(),
                members: vec![
                    member("zed_example", ""),
                    member("lin_example", "+"),
                    member("ada_example", ""),
                    member("Bo_example", "+@"),
                    member("grace_example", "@"),
                ],
            },
            1,
        );
        r
    }

    fn nicks(r: &Room) -> Vec<String> {
        r.members().iter().map(|m| m.nick.clone()).collect()
    }

    #[test]
    fn members_sort_ops_first() {
        let r = joined();
        assert_eq!(
            nicks(&r),
            [
                "Bo_example",
                "grace_example",
                "lin_example",
                "ada_example",
                "zed_example"
            ]
        );
        assert_eq!(r.member("bo_example").unwrap().prefixes, "@+");
        assert!(is_op("@+"));
        assert!(!is_op("+"));
    }

    #[test]
    fn joins_parts_and_modes_keep_the_list() {
        let mut r = joined();
        let a = r.apply(
            &Event::Join {
                channel: "#thicket".into(),
                nick: "cy_example".into(),
            },
            2,
        );
        assert!(a.line && !a.chat);
        assert_eq!(r.member_count(), 6);
        r.apply(
            &Event::MemberMode {
                channel: "#thicket".into(),
                nick: "cy_example".into(),
                prefix: '@',
                on: true,
            },
            2,
        );
        assert_eq!(nicks(&r)[0], "Bo_example");
        assert_eq!(r.member("cy_example").unwrap().prefixes, "@");
        r.apply(
            &Event::Nick {
                old: "cy_example".into(),
                new: "cy_away".into(),
            },
            3,
        );
        assert!(r.member("cy_away").is_some());
        r.apply(
            &Event::Quit {
                nick: "cy_away".into(),
                reason: "".into(),
            },
            4,
        );
        assert_eq!(r.member_count(), 5);
        let quiet = r.log().filter(|l| l.kind == LineKind::Membership).count();
        assert_eq!(quiet, 3);
        // A quit from someone not here isn't logged.
        let a = r.apply(
            &Event::Quit {
                nick: "elsewhere".into(),
                reason: "".into(),
            },
            5,
        );
        assert!(!a.line);
    }

    #[test]
    fn messages_mentions_and_private() {
        let mut r = joined();
        let a = r.apply(&msg("#thicket", "grace_example", "morning all"), 2);
        assert_eq!(
            a,
            Applied {
                line: true,
                chat: true,
                mention: false
            }
        );
        let a = r.apply(
            &msg("#thicket", "grace_example", "ada_example: see this"),
            3,
        );
        assert!(a.mention);
        let a = r.apply(&msg("ada_example", "grace_example", "psst"), 4);
        assert!(a.mention && a.chat);
        assert!(r.log().last().unwrap().private);
        // Another channel: ignored.
        assert!(!r.apply(&msg("#other", "grace_example", "hi"), 5).line);
        // NickServ's notices to us aren't chat.
        let ns = Event::Message(ChatMessage {
            target: "ada_example".into(),
            from: "NickServ".into(),
            text: "You are now identified".into(),
            kind: MsgKind::Notice,
            own: false,
        });
        assert!(!r.apply(&ns, 6).line);
        // Our own words never mention us.
        let own = Event::Message(ChatMessage {
            target: "#thicket".into(),
            from: "ada_example".into(),
            text: "ada_example here".into(),
            kind: MsgKind::Message,
            own: true,
        });
        assert_eq!(
            r.apply(&own, 7),
            Applied {
                line: true,
                chat: false,
                mention: false
            }
        );
    }

    #[test]
    fn the_log_is_bounded() {
        let mut r = joined();
        for i in 0..12 {
            r.apply(&msg("#thicket", "grace_example", &format!("m{i}")), i);
        }
        assert_eq!(r.log_len(), 5);
        assert_eq!(r.log().next().unwrap().text, "m7");
    }

    #[test]
    fn being_kicked_or_parting_empties_the_list() {
        let mut r = joined();
        r.apply(
            &Event::Kick {
                channel: "#thicket".into(),
                nick: "ada_example".into(),
                by: "grace_example".into(),
                reason: "test".into(),
            },
            2,
        );
        assert_eq!(r.member_count(), 0);
        assert!(!r.joined);
        assert_eq!(
            r.log().last().unwrap().text,
            "was removed by grace_example (test)"
        );
    }

    #[test]
    fn topic_changes_are_logged_after_the_first() {
        let mut r = joined();
        let t = |s: &str| Event::Topic {
            channel: "#thicket".into(),
            topic: s.into(),
        };
        assert!(!r.apply(&t("first"), 1).line);
        assert!(r.apply(&t("second"), 2).line);
        assert_eq!(r.topic.as_deref(), Some("second"));
    }
}
