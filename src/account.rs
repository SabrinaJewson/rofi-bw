pub(crate) fn load(data_path: &Path) -> anyhow::Result<Option<String>> {
    let file_path = data_path.join(ACCOUNT_FILE_NAME);
    let mut email = match fs::read_to_string(&*file_path) {
        Ok(email) => email,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).context(format!("failed to read {}", file_path.display()))?,
    };
    email.truncate(email.trim_end().len());
    Ok(Some(email))
}

pub(crate) fn store(data_path: &Path, email: &str) -> anyhow::Result<()> {
    let file_path = data_path.join(ACCOUNT_FILE_NAME);
    fs_overwrite::overwrite(&*file_path, email)
        .with_context(|| format!("failed to write {}", file_path.display()))
        .context("failed to store account")?;

    Ok(())
}

pub(crate) fn log_out(data_path: &Path) -> anyhow::Result<()> {
    match fs::remove_file(data_path.join(ACCOUNT_FILE_NAME)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).context("failed to log out"),
    }
}

const ACCOUNT_FILE_NAME: &str = "account";

use crate::fs_overwrite;
use anyhow::Context as _;
use std::fs;
use std::io;
use std::path::Path;
