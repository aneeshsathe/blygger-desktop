//! The BXP extension around [`Vault`]: the `notes` library
//! (`extension/library.*`) and the "Save selection to notes" command. It
//! never calls a `burrow/*` item method; the folder is the only thing it
//! touches, and only once the user has granted `fs:<vault>`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use blyg_ext::Capability;
use blyg_ext::capability::expand_home;
use blyg_ext::ext::{Extension, HostClient, serve};
use blyg_ext::protocol::*;
use blyg_ext::rpc::{params, to_value};
use serde_json::{Value, json};

use crate::vault::{Hit, Vault, VaultError};
use crate::{LIBRARY, SAVE_SELECTION, vault_setting};

/// How often the vault is re-scanned when `poll-ms` isn't set.
pub const DEFAULT_POLL: Duration = Duration::from_secs(2);

/// The extension's state.
#[derive(Default)]
pub struct NotesExt {
    vault: Option<Vault>,
    /// Why there's no vault (no grant, not a folder).
    problem: Option<RpcError>,
    folder: Option<String>,
    poll: Option<Duration>,
    granted: Vec<Capability>,
    home: Option<PathBuf>,
}

impl NotesExt {
    fn configure(&mut self, settings: &BTreeMap<String, String>) {
        let vault = vault_setting(settings);
        self.folder = settings
            .get("folder")
            .map(|f| f.trim().to_string())
            .filter(|f| !f.is_empty());
        self.poll = Some(
            settings
                .get("poll-ms")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(|ms| Duration::from_millis(ms.max(50)))
                .unwrap_or(DEFAULT_POLL),
        );
        let want = Capability::Fs(vault.clone());
        self.vault = None;
        self.problem = None;
        if !self.granted.iter().any(|g| want.covered_by(g)) {
            self.problem = Some(RpcError::permission_denied(&want.as_string()));
            return;
        }
        match Vault::open(expand_home(&vault, self.home.as_deref())) {
            Ok(v) => self.vault = Some(v),
            Err(e) => {
                self.problem = Some(RpcError::new(
                    codes::REFUSED,
                    format!("notes folder {vault}: {e}"),
                ))
            }
        }
    }

    fn vault(&mut self) -> Result<&mut Vault, RpcError> {
        match &mut self.vault {
            Some(v) => Ok(v),
            None => Err(self
                .problem
                .clone()
                .unwrap_or_else(|| RpcError::new(codes::REFUSED, "no notes folder"))),
        }
    }
}

fn check_library(l: &Option<String>) -> Result<(), RpcError> {
    match l.as_deref() {
        None | Some(LIBRARY) => Ok(()),
        Some(other) => Err(RpcError::invalid_params(format!("no library {other:?}"))),
    }
}

fn err(e: VaultError) -> RpcError {
    match e {
        VaultError::Stale { current } => {
            RpcError::new(codes::STALE, "the note changed on disk since it was read")
                .with_data(json!({ "currentHash": current }))
        }
        VaultError::Invalid(m) => RpcError::invalid_params(m),
        VaultError::NotFound => RpcError::new(codes::REFUSED, "no such note"),
        VaultError::Io(m) => RpcError::new(codes::REFUSED, m),
    }
}

fn entry(h: Hit) -> SourceEntry {
    SourceEntry {
        id: h.id,
        title: h.title,
        excerpt: h.excerpt,
        at: h.modified,
        url: None,
    }
}

impl Extension for NotesExt {
    fn initialize(
        &mut self,
        _host: &HostClient,
        p: InitializeParams,
    ) -> Result<InitializeResult, RpcError> {
        self.granted = p
            .granted
            .iter()
            .filter_map(|g| Capability::parse(g))
            .collect();
        self.home = blyg_ext::capability::home_dir();
        self.configure(&p.settings);
        Ok(InitializeResult {
            protocol_version: PROTOCOL_VERSION,
            name: crate::NAME.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            commands: vec![],
            sources: vec![],
            libraries: vec![],
        })
    }

    fn request(&mut self, _host: &HostClient, method: &str, p: Value) -> Result<Value, RpcError> {
        match method {
            methods::LIBRARY_LIST => {
                let p: LibraryListParams = params(p)?;
                check_library(&p.library)?;
                let entries = self.vault()?.list(p.path.as_deref()).map_err(err)?;
                let out: Vec<LibraryEntry> = entries
                    .into_iter()
                    .map(|e| LibraryEntry {
                        path: e.id.clone(),
                        id: e.id,
                        title: e.title,
                        is_dir: e.is_dir,
                        modified: e.modified,
                    })
                    .collect();
                to_value(&out)
            }
            methods::LIBRARY_SEARCH => {
                let p: LibrarySearchParams = params(p)?;
                check_library(&p.library)?;
                let hits: Vec<SourceEntry> = self
                    .vault()?
                    .search(&p.query, p.limit)
                    .into_iter()
                    .map(entry)
                    .collect();
                to_value(&hits)
            }
            methods::LIBRARY_READ => {
                let p: LibraryReadParams = params(p)?;
                check_library(&p.library)?;
                let n = self.vault()?.read(&p.id).map_err(err)?;
                to_value(&LibraryDocument {
                    id: n.id,
                    title: n.title,
                    markdown: n.markdown,
                    body: n.body,
                    hash: n.hash,
                })
            }
            methods::LIBRARY_WRITE => {
                let p: LibraryWriteParams = params(p)?;
                check_library(&p.library)?;
                let folder = p.folder.clone().or_else(|| self.folder.clone());
                let v = self.vault()?;
                let n = match &p.id {
                    Some(id) => {
                        let base = p.base_hash.as_deref().ok_or_else(|| {
                            RpcError::invalid_params("baseHash is needed to change a note")
                        })?;
                        v.save(id, &p.markdown, base)
                    }
                    None => v.create(folder.as_deref(), p.title.as_deref(), &p.markdown),
                }
                .map_err(err)?;
                to_value(&LibraryWritten {
                    id: n.id,
                    hash: n.hash,
                })
            }
            methods::COMMAND => {
                let p: CommandParams = params(p)?;
                if p.id != SAVE_SELECTION {
                    return Err(RpcError::method_not_found(&p.id));
                }
                let text = p.context.selection.unwrap_or_default();
                if text.trim().is_empty() {
                    return Err(RpcError::new(codes::REFUSED, "select some text first"));
                }
                let folder = self.folder.clone();
                let n = self
                    .vault()?
                    .create(folder.as_deref(), None, &text)
                    .map_err(err)?;
                to_value(&CommandResult {
                    toast: Some(format!("Saved to notes: {}", n.title)),
                    open: None,
                })
            }
            m => Err(RpcError::method_not_found(m)),
        }
    }

    fn notification(&mut self, _host: &HostClient, method: &str, p: Value) {
        if method == methods::SETTINGS_CHANGED
            && let Ok(p) = serde_json::from_value::<SettingsChanged>(p)
        {
            if !p.granted.is_empty() {
                self.granted = p
                    .granted
                    .iter()
                    .filter_map(|g| Capability::parse(g))
                    .collect();
            }
            self.configure(&p.settings);
        }
    }

    fn tick(&mut self, _host: &HostClient) {
        if let Some(v) = &mut self.vault {
            v.refresh();
        }
    }

    fn tick_interval(&self) -> Option<Duration> {
        self.poll
    }
}

/// Serve the extension over `read`/`write` until shutdown.
pub fn run<R, W>(read: R, write: W) -> ExitCode
where
    R: std::io::Read + Send + 'static,
    W: std::io::Write + Send + 'static,
{
    serve(read, write, &mut NotesExt::default());
    ExitCode::SUCCESS
}

/// [`run`] over this process's stdin/stdout (`blygger +ext markdown-notes`).
pub fn run_stdio() -> ExitCode {
    run(std::io::stdin(), std::io::stdout())
}
