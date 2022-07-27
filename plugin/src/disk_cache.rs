//! A simple filesystem-backed key-value cache.

pub(crate) struct DiskCache<Dir: Borrow<fs::Path>> {
    dir: Dir,
}

impl<Dir: Borrow<fs::Path>> DiskCache<Dir> {
    pub(crate) fn new(dir: Dir) -> Self {
        Self { dir }
    }

    pub(crate) fn load(&self, key: &str) -> anyhow::Result<Option<fs::PathBuf>> {
        self.load_inner(key)
            .with_context(|| format!("failed to load cache file {key}"))
    }

    fn load_inner(&self, key: &str) -> anyhow::Result<Option<fs::PathBuf>> {
        let dir = self.dir.borrow().join(key);
        let expires_path = dir.join("expires");

        let expires = match fs::read(&*expires_path) {
            Ok(file) => file,
            Err(fs::read::Error {
                kind: fs::read::ErrorKind::Open(e),
                ..
            }) if e.source.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let expires = (|| {
            let bytes = <[u8; 8]>::try_from(&*expires).ok()?;
            let secs = u64::from_le_bytes(bytes);
            SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(secs))
        })()
        .context("invalid expiration time")?;
        if expires < SystemTime::now() {
            drop(std::fs::remove_dir_all(&*dir));
            return Ok(None);
        }

        Ok(Some(dir.join("data")))
    }

    pub(crate) fn store(
        &self,
        key: &str,
        content: &[u8],
        expires: SystemTime,
    ) -> anyhow::Result<fs::PathBuf> {
        self.store_inner(key, content, expires)
            .with_context(|| format!("failed to store cache file {key}"))
    }

    fn store_inner(
        &self,
        key: &str,
        content: &[u8],
        expires: SystemTime,
    ) -> anyhow::Result<fs::PathBuf> {
        let dir = self.dir.borrow().join(key);

        let expires_secs = match expires.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(duration) => duration.as_secs(),
            // Saturate before the Unix epoch to zero seconds
            Err(_) => 0,
        };

        let data_path = dir.join("data");

        fs::overwrite::with(&*data_path, content)?;
        fs::overwrite::with(dir.join("expires"), &expires_secs.to_le_bytes())?;

        Ok(data_path)
    }
}

use anyhow::Context as _;
use rofi_bw_common::fs;
use std::borrow::Borrow;
use std::io;
use std::time::Duration;
use std::time::SystemTime;
