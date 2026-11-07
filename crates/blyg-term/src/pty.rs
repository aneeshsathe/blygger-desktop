//! The shell on a pty (wezterm's `portable-pty`: a Unix pty on macOS,
//! ConPTY on Windows). A reader thread hands the shell's output to a
//! callback; a writer thread sends keys; a waiter thread reports the exit.
//! Dropping a [`Pty`] kills the process it started (and only that one).

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// What to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// The whole environment (see [`crate::env::scrub`]); nothing else is
    /// inherited.
    pub env: Vec<(OsString, OsString)>,
}

impl ShellSpec {
    /// The user's login shell, or `override_cmd` (the `terminal-shell`
    /// setting: a program and its arguments, split on spaces), in the home
    /// directory, with the scrubbed environment.
    pub fn login(override_cmd: Option<&str>) -> ShellSpec {
        let (program, args) = match override_cmd
            .map(str::split_whitespace)
            .and_then(|mut w| Some((w.next()?.to_string(), w.map(str::to_string).collect())))
        {
            Some(p) => p,
            None => default_shell(),
        };
        ShellSpec {
            program,
            args,
            cwd: home(),
            env: crate::env::scrub(std::env::vars_os()),
        }
    }

    /// A one-off command (tests).
    pub fn command(program: &str, args: &[&str]) -> ShellSpec {
        ShellSpec {
            program: program.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: None,
            env: crate::env::scrub(std::env::vars_os()),
        }
    }

    /// The shell's name for the pane's header: `zsh`, `pwsh`, `cmd`.
    pub fn name(&self) -> String {
        let file = self
            .program
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&self.program);
        file.strip_suffix(".exe")
            .or_else(|| file.strip_suffix(".EXE"))
            .unwrap_or(file)
            .to_string()
    }
}

#[cfg(unix)]
fn default_shell() -> (String, Vec<String>) {
    // $SHELL, else the password database's (portable-pty looks both up).
    let shell = CommandBuilder::new_default_prog().get_shell();
    (shell, vec!["-l".into()])
}

#[cfg(windows)]
fn default_shell() -> (String, Vec<String>) {
    let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into());
    (shell, vec![])
}

fn home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

/// What the pty reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// Bytes the shell wrote.
    Bytes(Vec<u8>),
    /// The shell exited (may arrive twice: from the reader and the waiter).
    Exited,
}

/// A running shell.
pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: mpsc::Sender<Vec<u8>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    pid: Option<u32>,
    name: String,
}

impl Pty {
    /// Start `spec` on a new pty of `cols` × `rows`. `on_output` runs on the
    /// reader and waiter threads.
    pub fn spawn(
        spec: &ShellSpec,
        cols: u16,
        rows: u16,
        on_output: impl Fn(Output) + Send + Sync + 'static,
    ) -> std::io::Result<Pty> {
        let pair = native_pty_system().openpty(size(cols, rows)).map_err(io)?;
        let mut cmd = CommandBuilder::new(&spec.program);
        cmd.args(&spec.args);
        cmd.env_clear();
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        if let Some(dir) = &spec.cwd {
            cmd.cwd(dir);
        }
        let mut child = pair.slave.spawn_command(cmd).map_err(io)?;
        // The reader sees EOF once the child's side is closed.
        drop(pair.slave);
        let pid = child.process_id();
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().map_err(io)?;
        let mut writer = pair.master.take_writer().map_err(io)?;
        let on_output = Arc::new(on_output);

        let out = on_output.clone();
        std::thread::Builder::new()
            .name("terminal-read".into())
            .spawn(move || {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => out(Output::Bytes(buf[..n].to_vec())),
                    }
                }
                out(Output::Exited);
            })?;

        let out = on_output;
        std::thread::Builder::new()
            .name("terminal-wait".into())
            .spawn(move || {
                let _ = child.wait();
                out(Output::Exited);
            })?;

        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::Builder::new()
            .name("terminal-write".into())
            .spawn(move || {
                while let Ok(bytes) = rx.recv() {
                    if writer
                        .write_all(&bytes)
                        .and_then(|_| writer.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })?;

        Ok(Pty {
            master: pair.master,
            writer: tx,
            killer,
            pid,
            name: spec.name(),
        })
    }

    /// Send bytes to the shell (keys, pastes, the engine's replies).
    pub fn write(&self, bytes: Vec<u8>) {
        if !bytes.is_empty() {
            let _ = self.writer.send(bytes);
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.master.resize(size(cols, rows));
    }

    /// The shell's process id (the only process this pane ever kills).
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// The shell's name (`zsh`).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// End the shell.
    pub fn kill(&mut self) {
        let _ = self.killer.kill();
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        self.kill();
    }
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// portable-pty's errors (`anyhow::Error`) as I/O errors.
fn io(e: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::other(e.to_string())
}
