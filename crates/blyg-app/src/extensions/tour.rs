//! --- onboarding --- The tour's sample extensions. The bundled extensions
//! are off until the config names them, so a tour that showed the user's
//! own would show nothing on most Macs. While the tutorial runs, the
//! window's host is *parked* (like its backend) and a second host runs the
//! real bundled markdown-notes, reading-time and inspect over invented
//! notes in a temporary folder: two folders allowed and one not yet (its
//! chip in the warning colour), and the reading slots on the sample
//! reading list. cross-post isn't run: a macro would load a real website.
//!
//! Nothing the tour does reaches the config file: Allow, Turn on, Turn off,
//! Forget permissions and the notes folders' Add and Remove say what they
//! would do and leave the file alone (`ext_tour_refuses`). The tour host's
//! events have an inbox of their own: no consent question or notice from
//! it outlives the tour. When the tour ends its host stops, its folder is
//! deleted, and the user's host, library panel, reading slots and sheets
//! come back as they were.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use blyg_ext::{BackendApi, Capability, ExtEvent, Grants, Host, HostConfig};
use gpui_kit::*;

use super::{Launch, Library, NOTES, Overlay, reading_slots};
use crate::app::MainView;

/// A sample folder: (setting key, folder name, notes as (file, text),
/// allowed).
type SampleVault = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
    bool,
);

/// The sample folders.
const VAULTS: &[SampleVault] = &[
    (
        "vault",
        "Notes",
        &[
            (
                "Tide tables.md",
                "# Tide tables\n\nHigh water at 6:12, low just after noon. The harbour gulls \
                 know before the chart does.\n",
            ),
            (
                "Harbour walk.md",
                "---\ntags: [walks]\n---\n# Harbour walk\n\nThree boats in, two out. The fog \
                 horn sounded twice, then gave up.\n",
            ),
            (
                "Drafts/Fog.md",
                "# Fog\n\nFog rolls in the way a good sentence does: slowly, then all at once.\n",
            ),
        ],
        true,
    ),
    (
        "vault-garden",
        "Garden",
        &[
            (
                "Seed list.md",
                "# Seed list\n\n- runner beans\n- sweet peas\n- one stubborn artichoke\n",
            ),
            (
                "Compost.md",
                "# Compost\n\nTurn it on Sundays. Coffee grounds yes, citrus no.\n",
            ),
        ],
        true,
    ),
    (
        // (After Garden: labels sort, and the step says "Choose Garden".)
        "vault-shelf",
        "Shelf",
        &[(
            "Old letters.md",
            "# Old letters\n\nA box of postcards from the lighthouse.\n",
        )],
        false,
    ),
];

/// What the tour set aside.
struct Parked {
    host: Option<Host>,
    started: bool,
    lib: Library,
    slots: reading_slots::Slots,
    overlay: Option<Overlay>,
}

/// The tour's extensions, while the tutorial runs (`Extensions::tour`).
pub(crate) struct Tour {
    /// The sample vaults, as Settings and the drawer list them.
    pub vaults: Vec<blyg_ext_notes::VaultSpec>,
    dir: PathBuf,
    parked: Option<Parked>,
    /// Reload Config came while the tour ran: reload when it ends.
    reload: bool,
    inbox: Arc<Mutex<VecDeque<ExtEvent>>>,
}

/// Write the sample folders under `root`; their settings and grants.
fn write_vaults(root: &Path) -> std::io::Result<(BTreeMap<String, String>, Vec<Capability>)> {
    let mut settings = BTreeMap::new();
    let mut grants = vec![Capability::Ui];
    for (key, folder, notes, allowed) in VAULTS {
        let dir = root.join(folder);
        for (name, body) in *notes {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, body)?;
        }
        let text = dir.to_string_lossy().into_owned();
        if *allowed {
            grants.push(Capability::Fs(text.clone()));
        }
        settings.insert(key.to_string(), text);
    }
    Ok((settings, grants))
}

/// The tour host's configuration: markdown-notes on the sample folders,
/// reading-time and inspect, all from the app's own executable.
fn tour_config(launch: &Launch, root: &Path) -> std::io::Result<HostConfig> {
    let (notes, notes_grants) = write_vaults(&root.join("vaults"))?;
    let mut hc = HostConfig::new(root.join("data"), env!("CARGO_PKG_VERSION"));
    let rt = blyg_ext_reading_time::NAME;
    let inspect = blyg_ext_inspect::NAME;
    hc.enabled = vec![NOTES.into(), rt.into(), inspect.into()];
    let mut grants = Grants::default();
    for c in notes_grants {
        grants.grant(NOTES, c);
    }
    grants.grant(rt, Capability::ReadingRead);
    grants.grant(inspect, Capability::ReadingRead);
    hc.grants = grants;
    hc.bundled = vec![
        blyg_ext_notes::bundled(launch.program.clone(), launch.args.clone(), &notes),
        blyg_ext_crosspost::bundled(launch.program.clone(), launch.crosspost_args.clone()),
    ];
    hc.bundled.extend(reading_slots::bundled(launch));
    hc.settings.insert(NOTES.into(), notes);
    hc.timing = launch.timing.clone();
    Ok(hc)
}

impl MainView {
    /// The tutorial is running on the sample extensions.
    pub(crate) fn ext_tour_on(&self) -> bool {
        self.ext.tour.is_some()
    }

    /// The tutorial starts: park the user's extensions and run the sample
    /// ones. Without a launch (a test without one) the tour shows the
    /// user's host as it is.
    pub(crate) fn ext_tour_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ext.tour.is_some() {
            return;
        }
        // Tests run extensions only when they say how (a `Launch` global).
        let launch = if cfg!(test) {
            cx.try_global::<Launch>().cloned()
        } else {
            self.ext.launch.clone()
        };
        let Some(launch) = launch else {
            return;
        };
        let dir = std::env::temp_dir().join(format!(
            "burrow-tour-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let hc = match tour_config(&launch, &dir) {
            Ok(hc) => hc,
            Err(e) => {
                eprintln!("tour: couldn't write the sample notes: {e}");
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
        };
        let vaults = blyg_ext_notes::vaults(hc.settings.get(NOTES).unwrap_or(&BTreeMap::new()));
        let inbox: Arc<Mutex<VecDeque<ExtEvent>>> = Arc::default();
        let (wake, woken) = async_channel::bounded::<()>(1);
        let live = !cfg!(test);
        let sink = inbox.clone();
        let host = Host::new(
            hc,
            Arc::new(BackendApi::new(self.backend.clone())),
            move |ev| {
                sink.lock().unwrap_or_else(|p| p.into_inner()).push_back(ev);
                if live {
                    let _ = wake.try_send(());
                }
            },
        );
        self._tasks.push(cx.spawn_in(window, async move |this, cx| {
            while woken.recv().await.is_ok() {
                if this
                    .update_in(cx, |v, window, cx| v.ext_tour_pump(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        let parked = Parked {
            host: self.ext.host.replace(host.clone()),
            started: std::mem::replace(&mut self.ext.started, true),
            lib: std::mem::replace(&mut self.ext.lib, Library::for_tour()),
            slots: std::mem::take(&mut self.ext.slots),
            overlay: self.ext.overlay.take(),
        };
        self.ext.tour = Some(Tour {
            vaults,
            dir,
            parked: Some(parked),
            reload: false,
            inbox,
        });
        host.start();
        cx.notify();
    }

    /// The tutorial ends: stop the sample extensions, delete their folder,
    /// and put the user's back.
    pub(crate) fn ext_tour_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The tour's sheets go without a word (a question of the user's own
        // waits in the queue and is asked below).
        if let Some(Overlay::Consent { reply: Some(r), .. }) = self.ext.overlay.take() {
            r.answer(false);
        }
        self.ext.slots.sheet = None;
        let Some(mut tour) = self.ext.tour.take() else {
            return;
        };
        let Some(p) = tour.parked.take() else {
            return;
        };
        let sample = std::mem::replace(&mut self.ext.host, p.host);
        self.ext.started = p.started;
        self.ext.lib = p.lib;
        self.ext.slots = p.slots;
        self.ext.overlay = p.overlay;
        let dir = tour.dir.clone();
        cx.background_spawn(async move {
            if let Some(h) = sample {
                h.shutdown();
            }
            let _ = std::fs::remove_dir_all(dir);
        })
        .detach();
        if tour.reload {
            self.ext_reload(window, cx);
        }
        if self.ext.overlay.is_none() {
            self.ext_next_ask(window, cx);
        }
        cx.notify();
    }

    /// Reload Config while the tour runs: the user's host reloads when it
    /// ends. True when the tour took it.
    pub(crate) fn ext_tour_defer_reload(&mut self) -> bool {
        match self.ext.tour.as_mut() {
            Some(t) => {
                t.reload = true;
                true
            }
            None => false,
        }
    }

    /// In the tour: say what `what` would do, write nothing, and return
    /// true. Outside it, false.
    pub(crate) fn ext_tour_refuses(&mut self, what: String, cx: &mut Context<Self>) -> bool {
        if self.ext.tour.is_none() {
            return false;
        }
        self.show_toast(
            what,
            Some("Tour sample: your config file is unchanged".into()),
            cx,
        );
        cx.notify();
        true
    }

    /// Between the tour's steps: the palette, consent and Manage sheets and
    /// a ⋯ sheet go, and the drawer goes back to its reading notes. A
    /// question from the user's own host (shown from its notice meanwhile)
    /// is answered no, as Not now would.
    pub(crate) fn ext_tour_close_sheets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ext.overlay.is_some() {
            self.ext_close(window, cx);
        }
        self.ext.slots.sheet = None;
        self.ext.lib.tab = false;
    }

    /// The consent step's sheet: markdown-notes' first start, asking for
    /// two folders and messages (answering it writes nothing).
    pub(crate) fn ext_tour_consent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ext_show_consent(
            super::Ask {
                name: NOTES.into(),
                caps: vec![
                    Capability::Fs("~/Notes".into()),
                    Capability::Fs("~/Garden".into()),
                    Capability::Ui,
                ],
                reply: None,
            },
            window,
            cx,
        );
    }

    /// The tour host's events. Only what draws the sample (started,
    /// stopped, toasts, a note to open) is handled; a question it asks is
    /// answered no and leaves no notice behind.
    pub(crate) fn ext_tour_pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        loop {
            let ev = match self.ext.tour.as_ref() {
                Some(t) => t
                    .inbox
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .pop_front(),
                None => None,
            };
            let Some(ev) = ev else {
                break;
            };
            match ev {
                ExtEvent::Started { name, .. } => {
                    self.ext.slots.forget(&name);
                    self.ext_lib_refresh(window, cx);
                }
                ExtEvent::Toast { .. } | ExtEvent::OpenItem { .. } => {
                    self.ext_event(ev, window, cx)
                }
                ExtEvent::CapabilityRequested { reply, .. } => reply.answer(false),
                ExtEvent::BrowserPage { reply, .. } => reply.answer(None),
                ExtEvent::BrowserOpen { reply, .. } => {
                    reply.answer(Err("not during the tour".into()))
                }
                ExtEvent::Failed { name, message, .. } => {
                    eprintln!("tour: {name}: {message}");
                }
                ExtEvent::Stopped { .. } | ExtEvent::NeedsConsent { .. } => {}
            }
            cx.notify();
        }
    }
}
