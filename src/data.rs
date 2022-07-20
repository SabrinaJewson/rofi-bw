#[derive(Serialize, Deserialize)]
pub(crate) struct Data {
    pub(crate) email: Option<String>,
    pub(crate) device_id: Uuid,
}

pub(crate) fn load(data_path: &Path) -> anyhow::Result<Option<Data>> {
    let file_path = data_path.join(DATA_FILE_NAME);

    let data = match fs::read(&*file_path) {
        Ok(data) => data,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).context(format!("failed to read {}", file_path.display()))?,
    };

    let data = match &*data {
        [b' ', toml @ ..] => toml::from_slice::<Data>(toml).context("data file is invalid")?,
        [version, ..] => anyhow::bail!("Unsupported data version {version}"),
        [] => anyhow::bail!("Data file is empty"),
    };

    Ok(Some(data))
}

pub(crate) fn store(data_path: &Path, data: &Data) -> anyhow::Result<()> {
    let file_path = data_path.join(DATA_FILE_NAME);

    let mut buf = " ".to_owned();
    data.serialize(&mut toml::Serializer::new(&mut buf))
        .unwrap();

    fs_overwrite::overwrite(&*file_path, buf)
        .with_context(|| format!("failed to write {}", file_path.display()))
        .context("failed to write data file")?;

    Ok(())
}

const DATA_FILE_NAME: &str = "data";

use crate::fs_overwrite;
use anyhow::Context as _;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::io;
use std::path::Path;
use uuid::Uuid;
