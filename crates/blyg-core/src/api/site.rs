//! --- site --- The public site's settings (`crate::site`), the studio's
//! extensions (from its capabilities) and its update state (`GET
//! /api/update-state`, and the version `blyg.json` says it runs).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use super::Api;
use super::wire::WireSettings;
use crate::backend::{CoreError, Result};
use crate::site::{
    SiteConfig, StudioUpdate, generator_version, parse_extensions, parse_update_state,
};

/// How long an update state is kept before it's asked again. The studio
/// itself checks its release feed at most once a day.
const UPDATE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// The last update state per blyg (its base URL), and when it was read.
type UpdateCache = HashMap<String, (Instant, Option<StudioUpdate>)>;
static UPDATES: Mutex<Option<UpdateCache>> = Mutex::new(None);

impl Api {
    /// `GET /api/settings` as the site's settings, with the studio's
    /// extensions (its capabilities) and update state when it serves them.
    /// `Ok(None)`: no settings route.
    pub fn site_config(&self) -> Result<Option<SiteConfig>> {
        let Some(v) = Self::optional(self.call("GET", "/api/settings", None))? else {
            return Ok(None);
        };
        let mut cfg: SiteConfig = serde_json::from_value(v).map_err(|e| {
            CoreError::Other(format!("unexpected response from /api/settings: {e}"))
        })?;
        if let Ok(caps) = self.call("GET", "/api/capabilities", None) {
            cfg.server_extensions = parse_extensions(&caps);
        }
        if cfg.update_check != Some(false)
            && let Ok(u) = self.update_state()
        {
            self.cache_update(u.clone());
            cfg.update = u;
        }
        Ok(Some(cfg))
    }

    /// `PATCH /api/settings` with just `patch`; answers the stored site
    /// settings. A refusal is the server's error, as it said it.
    pub fn put_site_config(&self, patch: Map<String, Value>) -> Result<SiteConfig> {
        let v = self.call("PATCH", "/api/settings", Some(Value::Object(patch)))?;
        // What items resolve against (`siteUrl`) may have changed.
        match serde_json::from_value::<WireSettings>(v.clone()) {
            Ok(saved) => self.remember(Some(&saved.into())),
            Err(_) => *self.site.lock().unwrap() = None,
        }
        // An update-check change shows on the next read.
        if let Some(c) = UPDATES.lock().unwrap().as_mut() {
            c.remove(&self.base);
        }
        let mut cfg: SiteConfig = serde_json::from_value(v).map_err(|e| {
            CoreError::Other(format!("unexpected response from /api/settings: {e}"))
        })?;
        if let Ok(caps) = self.call("GET", "/api/capabilities", None) {
            cfg.server_extensions = parse_extensions(&caps);
        }
        Ok(cfg)
    }

    /// The studio's update state now: `GET /api/update-state` (which also
    /// starts the studio's own daily check), and the version it runs from
    /// `blyg.json`'s `generator`. `Ok(None)`: not served, or update alerts
    /// are off.
    pub fn fetch_studio_update(&self) -> Result<Option<StudioUpdate>> {
        let on = self
            .call("GET", "/api/settings", None)
            .ok()
            .and_then(|s| s.get("updateCheck").and_then(Value::as_bool));
        if on == Some(false) {
            return Ok(None);
        }
        self.update_state()
    }

    /// `GET /api/update-state` and the studio's version; `Ok(None)` on 404.
    fn update_state(&self) -> Result<Option<StudioUpdate>> {
        let Some(v) = Self::optional(self.call("GET", "/api/update-state", None))? else {
            return Ok(None);
        };
        Ok(Some(parse_update_state(&v, self.studio_version())))
    }

    fn cache_update(&self, u: Option<StudioUpdate>) {
        UPDATES
            .lock()
            .unwrap()
            .get_or_insert_with(HashMap::new)
            .insert(self.base.clone(), (Instant::now(), u));
    }

    /// The studio's version, from the public `blyg.json` (`generator`:
    /// `blygger-studio/0.56.0`). Unauthenticated: nothing is sent with it.
    pub fn studio_version(&self) -> Option<String> {
        let url = format!("{}/blyg.json", self.base);
        let v: Value = self
            .agent
            .get(&url)
            .set("accept", "application/json")
            .call()
            .ok()?
            .into_json()
            .ok()?;
        v.get("generator")
            .and_then(Value::as_str)
            .and_then(generator_version)
    }

    /// The update state as last read (never the network).
    pub fn cached_studio_update(&self) -> Option<StudioUpdate> {
        UPDATES
            .lock()
            .unwrap()
            .as_ref()?
            .get(&self.base)
            .and_then(|(_, u)| u.clone())
    }

    /// Read the update state again when it's older than six hours (or
    /// `force`). A failed read keeps what was known.
    pub fn refresh_studio_update(&self, force: bool) -> Result<Option<StudioUpdate>> {
        let fresh = UPDATES
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.get(&self.base))
            .filter(|(at, _)| !force && at.elapsed() < UPDATE_TTL)
            .map(|(_, u)| u.clone());
        if let Some(u) = fresh {
            return Ok(u);
        }
        let u = self.fetch_studio_update()?;
        self.cache_update(u.clone());
        Ok(u)
    }
}
