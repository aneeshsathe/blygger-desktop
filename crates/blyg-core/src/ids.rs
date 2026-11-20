//! Typed ids for other people's posts, so passing one where another is
//! expected doesn't compile.
//!
//! A reading post is named by two ids: the subscription it came through
//! ([`SubId`]) and the post's id on its blyg ([`RemoteId`]). Nine
//! `Backend` methods took both as `&str` (`mark_read`, `signal`,
//! `remote_versions`, `set_in_hopper`, …), so a swap compiled and would
//! have marked, signalled or filed the wrong post. Your own posts have
//! [`LocalId`](crate::LocalId) and [`ServerId`](crate::ServerId).
//!
//! The wire and the database keep the bare string: each type is
//! `#[serde(transparent)]` and goes through rusqlite as text, so nothing
//! about the stored or sent bytes changes (`tests` below prove it). The
//! types deref to `str`, so reading one as text (`starts_with`, `format!`,
//! a `&str` helper) needs nothing; only the typed seams, the `Backend`
//! trait, the store and the model fields, tell the two apart.

use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};

macro_rules! string_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(
            Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// The id as the wire and the database spell it.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_string(self) -> String {
                self.0
            }
        }

        /// Ids parsed from API rows, database rows and extension messages.
        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        impl From<&String> for $name {
            fn from(s: &String) -> Self {
                Self(s.clone())
            }
        }

        impl From<&$name> for $name {
            fn from(id: &$name) -> Self {
                id.clone()
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.0
            }
        }

        impl std::ops::Deref for $name {
            type Target = str;
            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        /// Lets a `HashMap<$name, _>` be looked up by `&str`; the hash is
        /// the string's, so this is sound.
        impl std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

        impl PartialEq<String> for $name {
            fn eq(&self, other: &String) -> bool {
                &self.0 == other
            }
        }

        impl PartialEq<$name> for str {
            fn eq(&self, other: &$name) -> bool {
                self == other.0
            }
        }

        impl PartialEq<$name> for &str {
            fn eq(&self, other: &$name) -> bool {
                *self == other.0
            }
        }

        impl PartialEq<$name> for String {
            fn eq(&self, other: &$name) -> bool {
                *self == other.0
            }
        }

        impl ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                self.0.to_sql()
            }
        }

        impl FromSql for $name {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                String::column_result(value).map(Self)
            }
        }
    };
}

string_id!(
    /// A subscription's id on your blyg (`Subscription::id`,
    /// `ReadingItem::subscription_id`). Empty on a row for one of your own
    /// posts shown as a reading row.
    SubId
);

string_id!(
    /// A reading post's id on the blyg that published it
    /// (`ReadingItem::remote_id`): unique within its subscription, so a
    /// post is the pair `(SubId, RemoteId)`.
    RemoteId
);

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &str = "01HZX3V8Q2W5M7N9P4R6T8Y0AB";

    /// JSON is byte-identical to the bare string, both ways, alone and as a
    /// field.
    #[test]
    fn serde_is_the_bare_string() {
        let sub = SubId::from(RAW);
        let remote = RemoteId::from(RAW);
        let bare = serde_json::to_string(RAW).unwrap();
        assert_eq!(serde_json::to_string(&sub).unwrap(), bare);
        assert_eq!(serde_json::to_string(&remote).unwrap(), bare);
        assert_eq!(serde_json::from_str::<SubId>(&bare).unwrap(), sub);
        assert_eq!(serde_json::from_str::<RemoteId>(&bare).unwrap(), remote);

        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Typed {
            sub: SubId,
            remote: Option<RemoteId>,
            pairs: Vec<(SubId, RemoteId)>,
        }
        #[derive(Serialize, Deserialize)]
        struct Bare {
            sub: String,
            remote: Option<String>,
            pairs: Vec<(String, String)>,
        }
        let typed = Typed {
            sub: sub.clone(),
            remote: Some(remote.clone()),
            pairs: vec![(sub, remote)],
        };
        let bare = Bare {
            sub: RAW.into(),
            remote: Some(RAW.into()),
            pairs: vec![(RAW.into(), RAW.into())],
        };
        let typed_json = serde_json::to_string(&typed).unwrap();
        assert_eq!(typed_json, serde_json::to_string(&bare).unwrap());
        assert_eq!(serde_json::from_str::<Typed>(&typed_json).unwrap(), typed);
    }

    /// SQLite stores the same TEXT value a `&str` would, and reads it back
    /// from a column a `&str` wrote.
    #[test]
    fn sql_is_the_bare_string() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE t (k TEXT, typed INTEGER)")
            .unwrap();
        let sub = SubId::from(RAW);
        let remote = RemoteId::from(RAW);
        db.execute("INSERT INTO t VALUES (?1, 0)", [RAW]).unwrap();
        db.execute("INSERT INTO t VALUES (?1, 1)", [&sub]).unwrap();
        db.execute("INSERT INTO t VALUES (?1, 2)", [&remote])
            .unwrap();
        let rows: Vec<(String, String, Vec<u8>)> = db
            .prepare("SELECT typeof(k), k, CAST(k AS BLOB) FROM t ORDER BY typed")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        for row in &rows {
            assert_eq!(row, &rows[0]);
        }
        assert_eq!(rows[0].0, "text");
        let back: (SubId, RemoteId) = db
            .query_row("SELECT k, k FROM t WHERE typed = 0", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(back, (sub, remote));
        // A lookup by a typed id finds the row a `&str` wrote.
        let n: i64 = db
            .query_row(
                "SELECT count(*) FROM t WHERE k = ?1",
                [&SubId::from(RAW)],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 3);
    }

    #[test]
    fn reads_as_text() {
        let id = RemoteId::from(RAW);
        assert_eq!(id.as_str(), RAW);
        assert_eq!(id.to_string(), RAW);
        assert!(id == RAW && RAW == id);
        let m: std::collections::HashMap<RemoteId, u8> = [(id, 1)].into();
        assert_eq!(m.get(RAW), Some(&1));
    }
}
