//! Checks owner-API traffic against blygger-studio's private API contract
//! 5.0.0 (`tests/fixtures/openapi.json`, studio v0.49.12). Covers the JSON
//! Schema keywords that file uses: `type` (incl. type lists), `$ref`,
//! `properties`, `required`, `additionalProperties`, `enum`, `anyOf`,
//! `allOf`, `items`, `minItems`, `maxItems`, `minLength`, `maxLength`,
//! `minimum`, `maximum`, `exclusiveMinimum`, and `pattern` for the one
//! pattern it uses (a `#rrggbb` colour). `format` is not checked.
//!
//! The contract's paths are relative to its server URL (`…/api`); callers
//! pass whole request paths (`["api", "items", …]`), so a contract whose
//! server URL has a path gets that path put in front of its templates.

use std::sync::OnceLock;

use serde_json::{Map, Value};

pub struct Contract {
    doc: Value,
    /// (METHOD, path template segments, operation)
    ops: Vec<(String, Vec<String>, Value)>,
}

/// The studio's private API contract (5.0.0).
pub fn contract() -> &'static Contract {
    static C: OnceLock<Contract> = OnceLock::new();
    C.get_or_init(|| Contract::load(include_str!("../fixtures/openapi.json")))
}

/// The `lineage-glyph` studio extension's owner reads (studio dc632c5).
pub fn lineage_glyph() -> &'static Contract {
    static C: OnceLock<Contract> = OnceLock::new();
    C.get_or_init(|| Contract::load(include_str!("../fixtures/lineage-glyph.json")))
}

impl Contract {
    fn load(text: &str) -> Contract {
        let doc: Value = serde_json::from_str(text).expect("contract JSON");
        // `servers[0].url`'s path (`/api`), put before every template.
        let prefix: Vec<String> = doc["servers"][0]["url"]
            .as_str()
            .and_then(|u| url::Url::parse(u).ok())
            .map(|u| {
                u.path()
                    .split('/')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let mut ops = vec![];
        for (path, item) in doc["paths"].as_object().unwrap() {
            let mut segs = prefix.clone();
            segs.extend(path.trim_start_matches('/').split('/').map(str::to_string));
            for (method, op) in item.as_object().unwrap() {
                ops.push((method.to_ascii_uppercase(), segs.clone(), op.clone()));
            }
        }
        Contract { doc, ops }
    }

    /// Every operation's `operationId`, in file order.
    pub fn operation_ids(&self) -> Vec<String> {
        self.ops
            .iter()
            .filter_map(|(_, _, op)| op["operationId"].as_str().map(str::to_string))
            .collect()
    }
}

fn is_param(seg: &str) -> bool {
    seg.starts_with('{') && seg.ends_with('}')
}

/// One part of a `multipart/form-data` body.
pub struct Part {
    pub name: String,
    pub content_type: Option<String>,
    pub data: Vec<u8>,
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (from..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

/// The parts of a `multipart/form-data` body, or `None` if it isn't one.
pub fn multipart(content_type: &str, body: &[u8]) -> Option<Vec<Part>> {
    let boundary = content_type
        .split(';')
        .map(str::trim)
        .find_map(|p| p.strip_prefix("boundary="))?
        .trim_matches('"');
    let delim = format!("--{boundary}").into_bytes();
    let mut parts = vec![];
    let mut at = find(body, &delim, 0)? + delim.len();
    loop {
        if body[at..].starts_with(b"--") {
            return Some(parts);
        }
        let head_start = at + 2; // CRLF after the delimiter
        let head_end = find(body, b"\r\n\r\n", head_start)?;
        let head = String::from_utf8_lossy(&body[head_start..head_end]).to_string();
        let next = find(body, &delim, head_end)?;
        let data = body[head_end + 4..next.saturating_sub(2)].to_vec();
        let mut name = None;
        let mut ctype = None;
        for line in head.split("\r\n") {
            let (k, v) = line.split_once(':')?;
            if k.trim().eq_ignore_ascii_case("content-disposition") {
                name = v
                    .split(';')
                    .map(str::trim)
                    .find_map(|p| p.strip_prefix("name="))
                    .map(|n| n.trim_matches('"').to_string());
            } else if k.trim().eq_ignore_ascii_case("content-type") {
                ctype = Some(v.trim().to_string());
            }
        }
        parts.push(Part {
            name: name?,
            content_type: ctype,
            data,
        });
        at = next + delim.len();
    }
}

impl Contract {
    /// The operation for `METHOD /path`; with several, the one with the
    /// most literal segments (`/items/{id}` loses to `/items/summary`).
    fn op(&self, method: &str, segs: &[&str]) -> Option<&Value> {
        self.ops
            .iter()
            .filter(|(m, t, _)| {
                m == method
                    && t.len() == segs.len()
                    && t.iter().zip(segs).all(|(t, s)| is_param(t) || t == s)
            })
            .max_by_key(|(_, t, _)| t.iter().filter(|s| !is_param(s)).count())
            .map(|(_, _, op)| op)
    }

    /// Whether `METHOD /path` is an operation here.
    pub fn knows(&self, method: &str, segs: &[&str]) -> bool {
        self.op(method, segs).is_some()
    }

    /// Whether this operation declares a reply with `status`.
    pub fn declares(&self, method: &str, segs: &[&str], status: u16) -> bool {
        self.op(method, segs)
            .is_some_and(|op| op["responses"].get(status.to_string()).is_some())
    }

    /// The request: the route exists, its query parameters are declared
    /// and valid, and the body matches the request schema (a JSON body
    /// fully; a multipart body by its field names).
    pub fn check_request(
        &self,
        method: &str,
        segs: &[&str],
        query: &[(String, String)],
        content_type: Option<&str>,
        body: &[u8],
    ) -> Result<(), String> {
        let op = self
            .op(method, segs)
            .ok_or_else(|| "not an operation in openapi.json".to_string())?;
        let params = op["parameters"].as_array().cloned().unwrap_or_default();
        for (k, v) in query {
            let p = params
                .iter()
                .find(|p| p["in"] == "query" && p["name"] == k.as_str())
                .ok_or_else(|| format!("undeclared query parameter `{k}`"))?;
            let schema = &p["schema"];
            let value = match serde_json::from_str::<Value>(v) {
                Ok(n @ Value::Number(_)) if self.allows_number(schema) => n,
                _ => Value::String(v.clone()),
            };
            self.validate(schema, &value, &format!("query.{k}"))?;
        }
        let rb = &op["requestBody"];
        let full_ct = content_type.unwrap_or("");
        let ct = full_ct.split(';').next().unwrap_or("").trim();
        if body.is_empty() {
            if rb["required"] == true {
                return Err("request body required".into());
            }
            return Ok(());
        }
        if rb.is_null() {
            return Err("this operation takes no request body".into());
        }
        let Some(media) = rb["content"].get(ct) else {
            return Err(format!("request content-type `{ct}` not accepted"));
        };
        let schema = self.resolve(&media["schema"]);
        match ct {
            "application/json" => {
                let v: Value =
                    serde_json::from_slice(body).map_err(|e| format!("request body: {e}"))?;
                self.validate(schema, &v, "body")?;
            }
            "multipart/form-data" => {
                let parts = multipart(full_ct, body).ok_or("body: not multipart")?;
                let props = schema["properties"].as_object();
                for p in &parts {
                    if props.is_none_or(|m| !m.contains_key(&p.name))
                        && schema["additionalProperties"] == false
                    {
                        return Err(format!("body: unknown field `{}`", p.name));
                    }
                }
                for k in schema["required"].as_array().into_iter().flatten() {
                    if !parts.iter().any(|p| Some(p.name.as_str()) == k.as_str()) {
                        return Err(format!("body: missing required `{k}`"));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The response: the status is declared for this operation, and the
    /// body matches its schema.
    pub fn check_response(
        &self,
        method: &str,
        segs: &[&str],
        status: u16,
        body: &Value,
    ) -> Result<(), String> {
        let op = self
            .op(method, segs)
            .ok_or_else(|| "not an operation in openapi.json".to_string())?;
        let Some(r) = op["responses"].get(status.to_string()) else {
            return Err(format!("status {status} not declared"));
        };
        let Some(schema) = r["content"]["application/json"].get("schema") else {
            return Ok(());
        };
        self.validate(schema, body, "response")
    }

    fn resolve<'a>(&'a self, s: &'a Value) -> &'a Value {
        match s.get("$ref").and_then(Value::as_str) {
            Some(r) => {
                let ptr = r.trim_start_matches('#');
                self.resolve(self.doc.pointer(ptr).expect("dangling $ref"))
            }
            None => s,
        }
    }

    fn allows_number(&self, s: &Value) -> bool {
        let s = self.resolve(s);
        match &s["type"] {
            Value::String(t) => t == "integer" || t == "number",
            Value::Array(ts) => ts.iter().any(|t| t == "integer" || t == "number"),
            _ => false,
        }
    }

    /// The properties `s` declares, `allOf` branches included (for the
    /// `additionalProperties: false` check of a composed schema).
    fn declared<'a>(&'a self, s: &'a Value, out: &mut Map<String, Value>) {
        let s = self.resolve(s);
        if let Some(p) = s["properties"].as_object() {
            out.extend(p.clone());
        }
        for b in s["allOf"].as_array().into_iter().flatten() {
            self.declared(b, out);
        }
    }

    pub fn validate(&self, s: &Value, v: &Value, at: &str) -> Result<(), String> {
        let s = self.resolve(s);
        if let Some(any) = s.get("anyOf").and_then(Value::as_array) {
            let errs: Vec<String> = any
                .iter()
                .filter_map(|alt| self.validate(alt, v, at).err())
                .collect();
            if errs.len() == any.len() {
                return Err(format!(
                    "{at}: matches no anyOf branch ({})",
                    errs.join(" | ")
                ));
            }
        }
        if let Some(all) = s.get("allOf").and_then(Value::as_array) {
            for b in all {
                self.validate(b, v, at)?;
            }
        }
        if let Some(e) = s.get("enum").and_then(Value::as_array)
            && !e.contains(v)
        {
            return Err(format!("{at}: {v} not in {}", Value::Array(e.clone())));
        }
        let types: Vec<&str> = match &s["type"] {
            Value::String(t) => vec![t.as_str()],
            Value::Array(ts) => ts.iter().filter_map(Value::as_str).collect(),
            _ => vec![],
        };
        if !types.is_empty() && !types.iter().any(|t| type_ok(t, v)) {
            return Err(format!("{at}: {v} is not {}", types.join("|")));
        }
        if let Some(n) = v.as_f64() {
            if let Some(m) = s["minimum"].as_f64()
                && n < m
            {
                return Err(format!("{at}: {n} < minimum {m}"));
            }
            if let Some(m) = s["maximum"].as_f64()
                && n > m
            {
                return Err(format!("{at}: {n} > maximum {m}"));
            }
            if let Some(m) = s["exclusiveMinimum"].as_f64()
                && n <= m
            {
                return Err(format!("{at}: {n} <= exclusiveMinimum {m}"));
            }
        }
        if let Some(t) = v.as_str() {
            let len = t.chars().count() as u64;
            if let Some(m) = s["minLength"].as_u64()
                && len < m
            {
                return Err(format!("{at}: shorter than {m}"));
            }
            if let Some(m) = s["maxLength"].as_u64()
                && len > m
            {
                return Err(format!("{at}: longer than {m}"));
            }
            if let Some(p) = s["pattern"].as_str()
                && !pattern_ok(p, t)
            {
                return Err(format!("{at}: {t:?} doesn't match {p}"));
            }
        }
        if let Some(a) = v.as_array() {
            if let Some(m) = s["maxItems"].as_u64()
                && a.len() as u64 > m
            {
                return Err(format!("{at}: more than {m} items"));
            }
            if let Some(m) = s["minItems"].as_u64()
                && (a.len() as u64) < m
            {
                return Err(format!("{at}: fewer than {m} items"));
            }
            if let Some(items) = s.get("items") {
                for (i, x) in a.iter().enumerate() {
                    self.validate(items, x, &format!("{at}[{i}]"))?;
                }
            }
        }
        if let Some(o) = v.as_object() {
            if let Some(req) = s["required"].as_array() {
                for k in req.iter().filter_map(Value::as_str) {
                    if !o.contains_key(k) {
                        return Err(format!("{at}: missing required `{k}`"));
                    }
                }
            }
            let mut props = Map::new();
            self.declared(s, &mut props);
            for (k, x) in o {
                match props.get(k) {
                    Some(ps) => self.validate(ps, x, &format!("{at}.{k}"))?,
                    None => match s.get("additionalProperties") {
                        Some(Value::Bool(false)) => {
                            return Err(format!("{at}: unknown field `{k}`"));
                        }
                        Some(ap @ Value::Object(m)) if !m.is_empty() => {
                            self.validate(ap, x, &format!("{at}.{k}"))?
                        }
                        _ => {}
                    },
                }
            }
        }
        Ok(())
    }
}

/// `pattern`, for the patterns the contract uses (no regex engine here).
fn pattern_ok(p: &str, t: &str) -> bool {
    match p {
        "^#[0-9a-fA-F]{6}$" => {
            t.len() == 7 && t.starts_with('#') && t[1..].chars().all(|c| c.is_ascii_hexdigit())
        }
        _ => true,
    }
}

fn type_ok(t: &str, v: &Value) -> bool {
    match t {
        "null" => v.is_null(),
        "boolean" => v.is_boolean(),
        "string" => v.is_string(),
        "number" => v.is_number(),
        "integer" => v.as_f64().is_some_and(|n| n.fract() == 0.0),
        "array" => v.is_array(),
        "object" => v.is_object(),
        _ => true,
    }
}
