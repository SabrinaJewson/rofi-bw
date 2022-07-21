pub(crate) struct Data {
    pub(crate) email: Option<String>,
    pub(crate) device_id: Uuid,
    path: fs::PathBuf,
}

impl Data {
    pub(crate) fn load(data_dir: &fs::Path) -> anyhow::Result<Self> {
        let path = data_dir.join("data");

        let bytes = match fs::read(&*path) {
            Ok(bytes) => bytes,
            Err(fs::read::Error {
                kind: fs::read::ErrorKind::Open(e),
                ..
            }) if e.source.kind() == io::ErrorKind::NotFound => {
                let this = Data {
                    email: None,
                    device_id: Uuid::new_v4(),
                    path,
                };
                this.store()?;
                return Ok(this);
            }
            Err(e) => return Err(e.into()),
        };

        #[derive(Deserialize)]
        struct Stored {
            email: Option<String>,
            device_id: Uuid,
        }

        let stored = match &*bytes {
            [versions::V0, toml @ ..] => {
                toml::from_slice::<Stored>(toml).context("data file is invalid")?
            }
            &[version, ..] => {
                anyhow::bail!("unknown version {:?} in data file", char::from(version))
            }
            [] => anyhow::bail!("data file is empty"),
        };

        Ok(Self {
            email: stored.email,
            device_id: stored.device_id,
            path,
        })
    }

    pub(crate) fn store(&self) -> anyhow::Result<()> {
        #[derive(Serialize)]
        struct Stored<'email> {
            email: Option<&'email str>,
            device_id: Uuid,
        }

        let mut buf = String::from(char::from(versions::V0));
        Stored {
            email: self.email.as_deref(),
            device_id: self.device_id,
        }
        .serialize(&mut toml::Serializer::new(&mut buf))
        .unwrap();

        fs::overwrite::with(&*self.path, buf.as_bytes()).context("failed to write data file")?;

        Ok(())
    }
}

mod versions {
    // Only UTF-8-compatible bytes are used because `toml` only supports serializing when appending
    // to strings.
    pub(crate) const V0: u8 = b'\0';
}

use anyhow::Context as _;
use rofi_bw_common::fs;
use serde::Deserialize;
use serde::Serialize;
use std::io;
use uuid::Uuid;
