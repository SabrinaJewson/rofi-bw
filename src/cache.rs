pub(crate) struct Cache {
    pub(crate) refresh_token: Box<str>,
    pub(crate) prelogin: Prelogin,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CacheRef<'refresh_token, 'prelogin> {
    pub(crate) refresh_token: &'refresh_token str,
    pub(crate) prelogin: &'prelogin Prelogin,
}

pub(crate) struct Key(Zeroizing<[u8; 32]>);

impl Key {
    pub(crate) fn new(email: &str, master_password: &str) -> Result<Self, KeyError> {
        let hasher = Argon2::default();
        let mut key = Zeroizing::new([0; 32]);
        hasher
            .hash_password_into(master_password.as_bytes(), email.as_bytes(), &mut *key)
            .map_err(KeyError)?;
        Ok(Self(key))
    }
    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new((&*self.0).into())
    }
}

#[derive(Debug)]
pub(crate) struct KeyError(argon2::Error);

impl Display for KeyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("failed to hash password")
    }
}

impl std::error::Error for KeyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

pub(crate) fn load(dir_path: &fs::Path, key: &Key) -> Option<Cache> {
    load_inner(dir_path, key).unwrap_or_else(|e| {
        eprintln!("Warning: {:?}", e.context("failed to load cache"));
        None
    })
}

fn load_inner(dir_path: &fs::Path, key: &Key) -> anyhow::Result<Option<Cache>> {
    let file_path = dir_path.join(CACHE_FILE_NAME);
    let data = match fs::read(&*file_path) {
        Ok(data) => data,
        Err(fs::read::Error {
            kind: fs::read::ErrorKind::Open(e),
            ..
        }) if e.source.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let data = match &*data {
        [0, rest @ ..] => rest,
        [version, ..] => anyhow::bail!("unsupported format version {version}"),
        [] => anyhow::bail!("refresh token cache file is empty"),
    };

    anyhow::ensure!(data.len() > 24, "file too short");

    let (nonce, ciphertext) = data.split_at(24);

    let decrypted = key
        .cipher()
        .decrypt(nonce.into(), ciphertext)
        .ok()
        .context("decryption failed")?;

    let cache = Reader::parse(&*decrypted, |reader| {
        let [token_len] = reader.read_array()?;
        let refresh_token = reader.read_utf8(usize::from(token_len))?;

        let [kdf_algorithm] = reader.read_array()?;
        let prelogin = match kdf_algorithm {
            0 => Prelogin::Pbkdf2 {
                algorithm: Pbkdf2Algorithm::Sha256,
                iterations: NonZeroU32::new(u32::from_le_bytes(reader.read_array()?))
                    .context("PBKDF2 required >0 iterations")?,
            },
            _ => anyhow::bail!("unknown hashing algorithm {kdf_algorithm}"),
        };

        Ok(Cache {
            refresh_token: refresh_token.into(),
            prelogin,
        })
    })?;

    Ok(Some(cache))
}

pub(crate) fn store(dir_path: &fs::Path, key: &Key, data: CacheRef<'_, '_>) {
    if let Err(e) = store_inner(dir_path, key, data) {
        eprintln!("Warning: {:?}", e.context("failed to store refresh token"));
    }
}

fn store_inner(dir_path: &fs::Path, key: &Key, data: CacheRef<'_, '_>) -> anyhow::Result<()> {
    let mut plaintext = Vec::new();
    let refresh_token_len = data.refresh_token.len();
    let refresh_token_len: u8 = refresh_token_len
        .try_into()
        .ok()
        .context("refresh token too long")?;
    plaintext.push(refresh_token_len);
    plaintext.extend_from_slice(data.refresh_token.as_bytes());
    match data.prelogin {
        Prelogin::Pbkdf2 {
            algorithm: Pbkdf2Algorithm::Sha256,
            iterations,
        } => {
            plaintext.push(0);
            plaintext.extend_from_slice(&iterations.get().to_le_bytes());
        }
    }

    let mut res = vec![0];

    let nonce = rand::random::<[u8; 24]>();

    res.extend_from_slice(&nonce);

    let ciphertext = key
        .cipher()
        .encrypt(&nonce.into(), &*plaintext)
        .expect("encryption cannot fail as `Vec`s are infallible");
    res.extend_from_slice(&ciphertext);

    fs::overwrite::with(dir_path.join(CACHE_FILE_NAME), &*res).context("failed to write cache")?;

    Ok(())
}

const CACHE_FILE_NAME: &str = "cache";

struct Reader<'source>(&'source [u8]);
impl<'source> Reader<'source> {
    fn parse<O, F>(source: &'source [u8], parser: F) -> anyhow::Result<O>
    where
        F: FnOnce(&mut Reader<'source>) -> anyhow::Result<O>,
    {
        let mut this = Self(source);
        let res = parser(&mut this)?;
        this.finish()?;
        Ok(res)
    }
    fn read(&mut self, n: usize) -> anyhow::Result<&'source [u8]> {
        anyhow::ensure!(self.0.len() >= n, "unexpected EOF");
        let (start, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(start)
    }
    fn read_utf8(&mut self, n: usize) -> anyhow::Result<&'source str> {
        str::from_utf8(self.read(n)?).context("string was not valid UTF-8")
    }
    fn read_array<const N: usize>(&mut self) -> anyhow::Result<[u8; N]> {
        Ok(self.read(N)?.try_into().unwrap())
    }
    fn finish(self) -> anyhow::Result<()> {
        anyhow::ensure!(self.0.is_empty(), "trailing bytes");
        Ok(())
    }
}

use crate::auth::Pbkdf2Algorithm;
use crate::auth::Prelogin;
use aead::Aead;
use aead::NewAead;
use anyhow::Context as _;
use argon2::Argon2;
use chacha20poly1305::XChaCha20Poly1305;
use rofi_bw_common::fs;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io;
use std::num::NonZeroU32;
use std::str;
use zeroize::Zeroizing;
