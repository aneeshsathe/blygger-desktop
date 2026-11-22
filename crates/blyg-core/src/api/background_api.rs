//! --- background --- The owner-API calls for Background and Foreground
//! (studio 0.61.0, contract 5.10.0; `crate::background`). Asked only of a
//! studio whose capabilities list `setReadingPlacement`.

use serde_json::{Value, json};

use super::{Api, CURSOR_RESTARTS, enc, is_transient};
use crate::backend::{CoreError, Result};
use crate::background::{
    BackgroundRules, Key, LENS_LIMIT, LENS_QUERY, Placement, placement_body, serves,
};

/// Pages of the Background lens read at most (50 entries each).
const MAX_PAGES: usize = 200;

impl Api {
    /// Whether this studio has Background: its capabilities' `operations`
    /// list `setReadingPlacement`. A studio without capabilities (404, or
    /// not readable) doesn't; a failed read is an error.
    pub fn serves_background(&self) -> Result<bool> {
        match self.call("GET", "/api/capabilities", None) {
            Ok(v) => Ok(serves(&v)),
            Err(e) if is_transient(&e) || matches!(e, CoreError::Unauthorized) => Err(e),
            Err(_) => Ok(false),
        }
    }

    /// Every entry of the studio's Background lens (`GET
    /// /api/reading?background=true`, every page), as served. One coherent
    /// acquisition: a cursor that expires mid-walk starts it over.
    pub fn background_lens(&self) -> Result<Vec<Value>> {
        for _ in 0..=CURSOR_RESTARTS {
            match self.background_walk() {
                Err(CoreError::CursorExpired) => continue,
                r => return r,
            }
        }
        Err(CoreError::CursorExpired)
    }

    fn background_walk(&self) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut path = format!("/api/reading?{LENS_QUERY}&limit={LENS_LIMIT}");
            if let Some(c) = &cursor {
                path.push_str(&format!("&cursor={}", enc(c)));
            }
            let v = self.call("GET", &path, None)?;
            if let Some(items) = v.get("items").and_then(Value::as_array) {
                out.extend(items.iter().cloned());
            }
            let next = v
                .get("page")
                .and_then(|p| p.get("nextCursor"))
                .and_then(Value::as_str)
                .map(str::to_string);
            match next {
                Some(n) if cursor.as_deref() == Some(n.as_str()) => {
                    return Err(CoreError::Other(
                        "/api/reading answered the same page twice".into(),
                    ));
                }
                Some(n) => cursor = Some(n),
                None => return Ok(out),
            }
        }
        Err(CoreError::Other(
            "the Background lens ran past its page budget".into(),
        ))
    }

    /// `PUT /api/reading/placement`: the owner's choice on one entry (your
    /// own posts as `("~own", id)`); `None` clears it and the rules decide.
    pub fn set_reading_placement(&self, key: &Key, placement: Option<Placement>) -> Result<()> {
        self.call(
            "PUT",
            "/api/reading/placement",
            Some(placement_body(key, placement)),
        )
        .map(|_| ())
    }

    /// `PATCH /api/subscriptions/:id {background}`: move a whole blyg to
    /// Background, or back.
    pub fn set_subscription_background(&self, id: &str, on: bool) -> Result<()> {
        self.call(
            "PATCH",
            &format!("/api/subscriptions/{}", enc(id)),
            Some(json!({ "background": on })),
        )
        .map(|_| ())
    }

    /// The `backgroundRules` of `GET /api/settings`. `Ok(None)`: not served.
    pub fn background_rules(&self) -> Result<Option<BackgroundRules>> {
        if !self.serves_background()? {
            return Ok(None);
        }
        let v: Option<Value> = Self::optional(self.call("GET", "/api/settings", None))?;
        Ok(v.and_then(|v| BackgroundRules::from_settings(&v)))
    }

    /// `PATCH /api/settings {backgroundRules: {rule: on}}`: that one rule
    /// only (the studio merges it into what it stores); the rules back.
    pub fn put_background_rule(&self, rule: &str, on: bool) -> Result<BackgroundRules> {
        let v = self.call(
            "PATCH",
            "/api/settings",
            Some(BackgroundRules::patch(rule, on)),
        )?;
        BackgroundRules::from_settings(&v)
            .ok_or_else(|| CoreError::Other("your blyg didn't keep the Background rules".into()))
    }

    /// --- reader order --- The `readerOrder` of `GET /api/settings`
    /// (studio 0.68, contract 6.1.0). `Ok(None)`: the studio doesn't have
    /// it (a setting, not an operation: its absence is the gate).
    pub fn reader_order(&self) -> Result<Option<crate::order::ReaderOrder>> {
        let v: Option<Value> = Self::optional(self.call("GET", "/api/settings", None))?;
        Ok(v.and_then(|v| crate::order::ReaderOrder::from_settings(&v)))
    }

    /// `PATCH /api/settings {readerOrder}`: that key only; the stored value
    /// back. Sent only to a studio whose settings had the key (an older one
    /// refuses an unknown key with a 400).
    pub fn put_reader_order(
        &self,
        order: crate::order::ReaderOrder,
    ) -> Result<crate::order::ReaderOrder> {
        let v = self.call("PATCH", "/api/settings", Some(order.patch()))?;
        crate::order::ReaderOrder::from_settings(&v)
            .ok_or_else(|| CoreError::Other("your blyg didn't keep the reader order".into()))
    }
}
