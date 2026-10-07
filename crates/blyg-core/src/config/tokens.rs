//! Owner-credential storage. No credential ever goes in a file: they live
//! in the macOS Keychain under service `org.blygger.desktop`, account =
//!
//! - `<host> oauth`: a browser sign-in's grant (JSON: tokens, expiry,
//!   where to renew it);
//! - `<host>`: an API token;
//! - `<host> password`: the studio password;
//! - `<host> oauth-client`: the client id Burrow registered with that blyg
//!   (not a secret; kept across sign-outs so it's reused).
//!
//! A blyg has one credential at a time; when more than one is found, the
//! browser sign-in wins, then the token, then the password. Access goes
//! through the `TokenStore` trait so tests use `MemoryTokenStore` and never
//! touch the real Keychain.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::api::auth::Credential;
use crate::api::oauth::{OAuthGrant, OAuthSession, grant_key};
use crate::backend::{CoreError, Result};

pub const KEYCHAIN_SERVICE: &str = "org.blygger.desktop";

/// Keychain account for a base URL: its host (`blyg.example.com`).
pub fn token_account(base_url: &str) -> String {
    url::Url::parse(base_url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| base_url.trim().to_string())
}

pub trait TokenStore: Send + Sync {
    fn get(&self, base_url: &str) -> Result<Option<String>>;
    fn set(&self, base_url: &str, token: &str) -> Result<()>;
    fn delete(&self, base_url: &str) -> Result<()>;
}

/// The `TokenStore` key a blyg's studio password is kept under: its own
/// Keychain account, `<host> password`, next to the token's `<host>`.
pub fn password_key(base_url: &str) -> String {
    format!("{} password", token_account(base_url))
}

/// The browser sign-in kept for a blyg, if any (also when it has ended).
pub fn load_grant(store: &dyn TokenStore, base_url: &str) -> Result<Option<OAuthGrant>> {
    Ok(store
        .get(&grant_key(base_url))?
        .and_then(|s| serde_json::from_str(&s).ok()))
}

/// The credential stored for a blyg: its browser sign-in, else its API
/// token, else its studio password. A browser sign-in renews itself and
/// saves each renewal back to `store`.
pub fn load_credential(store: &Arc<dyn TokenStore>, base_url: &str) -> Result<Option<Credential>> {
    if let Some(g) = load_grant(store.as_ref(), base_url)? {
        return Ok(Some(Credential::OAuth(OAuthSession::kept(
            g,
            store.clone(),
            base_url,
        ))));
    }
    if let Some(t) = store.get(base_url)? {
        return Ok(Some(Credential::Token(t)));
    }
    Ok(store
        .get(&password_key(base_url))?
        .map(Credential::Password))
}

/// Keep `cred` for a blyg, and drop the other kinds, so there's one.
pub fn save_credential(store: &dyn TokenStore, base_url: &str, cred: &Credential) -> Result<()> {
    let (keep, value) = match cred {
        Credential::Token(t) => (base_url.to_string(), t.clone()),
        Credential::Password(p) => (password_key(base_url), p.clone()),
        Credential::OAuth(s) => (
            grant_key(base_url),
            serde_json::to_string(&s.grant()).map_err(|e| CoreError::Other(e.to_string()))?,
        ),
    };
    store.set(&keep, &value)?;
    for key in credential_keys(base_url) {
        if key != keep {
            store.delete(&key)?;
        }
    }
    Ok(())
}

/// Forget every credential kept for a blyg (the grant, the token and the
/// password). The registered client id stays, to be reused.
pub fn delete_credential(store: &dyn TokenStore, base_url: &str) -> Result<()> {
    for key in credential_keys(base_url) {
        store.delete(&key)?;
    }
    Ok(())
}

fn credential_keys(base_url: &str) -> [String; 3] {
    [
        grant_key(base_url),
        base_url.to_string(),
        password_key(base_url),
    ]
}

/// In-memory token store (tests, `BLYGGER_FAKE`).
#[derive(Default)]
pub struct MemoryTokenStore(Mutex<HashMap<String, String>>);

impl TokenStore for MemoryTokenStore {
    fn get(&self, base_url: &str) -> Result<Option<String>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| CoreError::Other("poisoned".into()))?
            .get(&token_account(base_url))
            .cloned())
    }
    fn set(&self, base_url: &str, token: &str) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| CoreError::Other("poisoned".into()))?
            .insert(token_account(base_url), token.to_string());
        Ok(())
    }
    fn delete(&self, base_url: &str) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| CoreError::Other("poisoned".into()))?
            .remove(&token_account(base_url));
        Ok(())
    }
}

/// The real macOS Keychain (feature `keychain`, on by default).
#[cfg(feature = "keychain")]
#[derive(Debug, Default, Clone, Copy)]
pub struct KeychainTokenStore;

#[cfg(feature = "keychain")]
impl KeychainTokenStore {
    fn entry(base_url: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(KEYCHAIN_SERVICE, &token_account(base_url))
            .map_err(|e| CoreError::Other(format!("keychain: {e}")))
    }
}

#[cfg(feature = "keychain")]
impl TokenStore for KeychainTokenStore {
    fn get(&self, base_url: &str) -> Result<Option<String>> {
        match Self::entry(base_url)?.get_password() {
            Ok(t) => Ok(Some(t)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CoreError::Other(format!("keychain: {e}"))),
        }
    }
    fn set(&self, base_url: &str, token: &str) -> Result<()> {
        Self::entry(base_url)?
            .set_password(token)
            .map_err(|e| CoreError::Other(format!("keychain: {e}")))
    }
    fn delete(&self, base_url: &str) -> Result<()> {
        match Self::entry(base_url)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CoreError::Other(format!("keychain: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_are_kept_apart_and_one_at_a_time() {
        let mem = Arc::new(MemoryTokenStore::default());
        let s: Arc<dyn TokenStore> = mem.clone();
        let base = "https://blyg.example.com/";
        assert_eq!(password_key(base), "blyg.example.com password");
        assert_eq!(grant_key(base), "blyg.example.com oauth");
        assert_eq!(load_credential(&s, base).unwrap(), None);
        save_credential(&*s, base, &Credential::Token("t".into())).unwrap();
        assert_eq!(
            load_credential(&s, base).unwrap(),
            Some(Credential::Token("t".into()))
        );
        save_credential(&*s, base, &Credential::Password("p".into())).unwrap();
        assert_eq!(
            load_credential(&s, base).unwrap(),
            Some(Credential::Password("p".into()))
        );
        assert_eq!(s.get(base).unwrap(), None, "the token is dropped");
        let grant = OAuthGrant {
            access: "a".into(),
            refresh: Some("r".into()),
            expires: 1,
            deadline: 0,
            scope: "owner:read".into(),
            client_id: "cid".into(),
            token_endpoint: "https://blyg.example.com/t".into(),
            revocation_endpoint: None,
            resource: "https://blyg.example.com/api".into(),
            invalid: false,
        };
        save_credential(
            &*s,
            base,
            &Credential::OAuth(OAuthSession::new(grant.clone())),
        )
        .unwrap();
        assert_eq!(
            s.get(&password_key(base)).unwrap(),
            None,
            "the password is dropped"
        );
        match load_credential(&s, base).unwrap() {
            Some(Credential::OAuth(o)) => assert_eq!(o.grant(), grant),
            other => panic!("{other:?}"),
        }
        // More than one kept (an older version): browser, token, password.
        s.set(base, "t").unwrap();
        s.set(&password_key(base), "p").unwrap();
        assert!(matches!(
            load_credential(&s, base).unwrap(),
            Some(Credential::OAuth(_))
        ));
        s.delete(&grant_key(base)).unwrap();
        assert_eq!(
            load_credential(&s, base).unwrap(),
            Some(Credential::Token("t".into()))
        );
        s.set(&grant_key(base), &serde_json::to_string(&grant).unwrap())
            .unwrap();
        delete_credential(&*s, base).unwrap();
        assert_eq!(load_credential(&s, base).unwrap(), None);
    }

    #[test]
    fn token_account_is_host() {
        assert_eq!(
            token_account("https://blyg.example.com/"),
            "blyg.example.com"
        );
        assert_eq!(token_account("http://127.0.0.1:8787"), "127.0.0.1");
    }

    #[test]
    fn memory_token_store() {
        let s = MemoryTokenStore::default();
        assert_eq!(s.get("https://a.example").unwrap(), None);
        s.set("https://a.example/", "t0k").unwrap();
        assert_eq!(s.get("https://a.example").unwrap().as_deref(), Some("t0k"));
        s.delete("https://a.example").unwrap();
        assert_eq!(s.get("https://a.example").unwrap(), None);
    }
}
