//! The local model, fetched once when the user turns it on: each file
//! pinned to a commit of its Hugging Face repository, with its size and
//! sha256 checked before it's kept. After that it's read from disk and
//! nothing goes over the network.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};

use crate::{EmbedError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFile {
    pub name: &'static str,
    pub url: &'static str,
    pub size: u64,
    /// Lowercase hex.
    pub sha256: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelSpec {
    /// The directory under the models folder, and what vectors are
    /// stored with (`StaticModel::id`).
    pub id: &'static str,
    /// For people: the model's name, where it's from, its licence.
    pub name: &'static str,
    pub source: &'static str,
    pub licence: &'static str,
    pub files: &'static [ModelFile],
}

impl ModelSpec {
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// Where it lives under `models` (the data dir's `models/`).
    pub fn dir(&self, models: &Path) -> PathBuf {
        models.join(self.id)
    }

    /// Every file is there at its pinned size and was checked when it
    /// arrived (the `.verified` marker).
    pub fn installed(&self, models: &Path) -> bool {
        let dir = self.dir(models);
        dir.join(VERIFIED).is_file()
            && self
                .files
                .iter()
                .all(|f| std::fs::metadata(dir.join(f.name)).is_ok_and(|m| m.len() == f.size))
    }
}

const VERIFIED: &str = ".verified";

/// MinishLab's potion-base-8M (MIT): a 256-dimensional static embedding of
/// bge-base-en-v1.5's 29,528 tokens, distilled from that model. 30 MB.
pub const POTION_BASE_8M: ModelSpec = ModelSpec {
    id: "potion-base-8M@bf8b056",
    name: "potion-base-8M",
    source: "https://huggingface.co/minishlab/potion-base-8M",
    licence: "MIT",
    files: &[
        ModelFile {
            name: "model.safetensors",
            url: "https://huggingface.co/minishlab/potion-base-8M/resolve/\
                  bf8b056651a2c21b8d2565580b8569da283cab23/model.safetensors",
            size: 30_236_760,
            sha256: "f65d0f325faadc1e121c319e2faa41170d3fa07d8c89abd48ca5358d9a223de2",
        },
        ModelFile {
            name: "tokenizer.json",
            url: "https://huggingface.co/minishlab/potion-base-8M/resolve/\
                  bf8b056651a2c21b8d2565580b8569da283cab23/tokenizer.json",
            size: 683_666,
            sha256: "e67e803f624fb4d67dea1c730d06e1067e1b14d830e2c2202569e3ef0f70bb50",
        },
    ],
};

/// Where one file's bytes come from: the network, or (tests) anything.
pub trait Fetch {
    fn open(&self, url: &str) -> Result<Box<dyn Read + Send>>;
}

/// The network, over the same client stack as the rest of Burrow.
pub struct Http;

impl Fetch for Http {
    fn open(&self, url: &str) -> Result<Box<dyn Read + Send>> {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(20))
            .timeout_read(std::time::Duration::from_secs(60))
            .build();
        let r = agent
            .get(url)
            .call()
            .map_err(|e| EmbedError::Network(e.to_string()))?;
        Ok(Box::new(r.into_reader()))
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Fetch `spec` into `models`, reporting `(done, total)` bytes. Each file
/// is written beside its final name, checked (size, sha256), then moved
/// into place; a file that fails is deleted and nothing is marked
/// installed. `cancel` stops it between reads.
pub fn download(
    spec: &ModelSpec,
    models: &Path,
    fetch: &dyn Fetch,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<()> {
    let dir = spec.dir(models);
    std::fs::create_dir_all(&dir).map_err(|e| EmbedError::Model(e.to_string()))?;
    let _ = std::fs::remove_file(dir.join(VERIFIED));
    let total = spec.total_size();
    let mut done = 0u64;
    progress(0, total);
    for f in spec.files {
        let part = dir.join(format!("{}.part", f.name));
        let r = fetch_one(f, &part, fetch, cancel, &mut |n| {
            progress(done + n, total);
        });
        if let Err(e) = r {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
        std::fs::rename(&part, dir.join(f.name)).map_err(|e| EmbedError::Model(e.to_string()))?;
        done += f.size;
        progress(done, total);
    }
    std::fs::write(dir.join(VERIFIED), spec.id).map_err(|e| EmbedError::Model(e.to_string()))?;
    Ok(())
}

fn fetch_one(
    f: &ModelFile,
    part: &Path,
    fetch: &dyn Fetch,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    let io = |e: std::io::Error| EmbedError::Model(e.to_string());
    let mut src = fetch.open(f.url)?;
    let mut out = std::fs::File::create(part).map_err(io)?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut n = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(EmbedError::Model("download cancelled".into()));
        }
        let k = src
            .read(&mut buf)
            .map_err(|e| EmbedError::Network(e.to_string()))?;
        if k == 0 {
            break;
        }
        n += k as u64;
        if n > f.size {
            return Err(EmbedError::Model(format!(
                "{} is larger than expected",
                f.name
            )));
        }
        hash.update(&buf[..k]);
        out.write_all(&buf[..k]).map_err(io)?;
        progress(n);
    }
    out.flush().map_err(io)?;
    if n != f.size {
        return Err(EmbedError::Model(format!(
            "{} is {n} bytes, expected {}",
            f.name, f.size
        )));
    }
    let got = hex(&hash.finalize());
    if got != f.sha256 {
        return Err(EmbedError::Model(format!(
            "{} failed its checksum; nothing was kept",
            f.name
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(Vec<(&'static str, Vec<u8>)>);

    impl Fetch for Fixed {
        fn open(&self, url: &str) -> Result<Box<dyn Read + Send>> {
            let b = self
                .0
                .iter()
                .find(|(u, _)| *u == url)
                .map(|(_, b)| b.clone())
                .ok_or_else(|| EmbedError::Network("404".into()))?;
            Ok(Box::new(std::io::Cursor::new(b)))
        }
    }

    fn sha(b: &[u8]) -> String {
        hex(&Sha256::digest(b))
    }

    #[test]
    fn checks_size_and_checksum_before_keeping_a_file() {
        let tmp = tempfile::tempdir().unwrap();
        let a = b"hello model".to_vec();
        let b = b"{\"tokenizer\":1}".to_vec();
        let sha_a: &'static str = Box::leak(sha(&a).into_boxed_str());
        let sha_b: &'static str = Box::leak(sha(&b).into_boxed_str());
        let files: &'static [ModelFile] = Box::leak(Box::new([
            ModelFile {
                name: "model.safetensors",
                url: "mem://a",
                size: a.len() as u64,
                sha256: sha_a,
            },
            ModelFile {
                name: "tokenizer.json",
                url: "mem://b",
                size: b.len() as u64,
                sha256: sha_b,
            },
        ]));
        let spec = ModelSpec {
            id: "tiny@1",
            name: "tiny",
            source: "",
            licence: "MIT",
            files,
        };
        let no = AtomicBool::new(false);
        let mut seen = Vec::new();
        // A corrupted file: refused, nothing marked installed.
        let bad = Fixed(vec![
            ("mem://a", b"hello modem".to_vec()),
            ("mem://b", b.clone()),
        ]);
        let err = download(&spec, tmp.path(), &bad, &mut |d, t| seen.push((d, t)), &no);
        assert!(matches!(err, Err(EmbedError::Model(m)) if m.contains("checksum")));
        assert!(!spec.installed(tmp.path()));
        assert!(!spec.dir(tmp.path()).join("model.safetensors.part").exists());
        // The real thing.
        let good = Fixed(vec![("mem://a", a.clone()), ("mem://b", b.clone())]);
        download(&spec, tmp.path(), &good, &mut |d, t| seen.push((d, t)), &no).unwrap();
        assert!(spec.installed(tmp.path()));
        assert_eq!(seen.last(), Some(&(spec.total_size(), spec.total_size())));
        // Cancelled: stops, and the old marker is gone.
        let yes = AtomicBool::new(true);
        assert!(download(&spec, tmp.path(), &good, &mut |_, _| {}, &yes).is_err());
        assert!(!spec.installed(tmp.path()));
    }

    #[test]
    fn the_pinned_model_is_the_one_documented() {
        assert_eq!(POTION_BASE_8M.licence, "MIT");
        assert_eq!(POTION_BASE_8M.total_size(), 30_920_426);
        for f in POTION_BASE_8M.files {
            assert!(
                f.url
                    .contains("/resolve/bf8b056651a2c21b8d2565580b8569da283cab23/")
            );
            assert!(f.url.ends_with(f.name));
            assert_eq!(f.sha256.len(), 64);
        }
    }
}
