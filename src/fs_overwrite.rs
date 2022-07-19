pub(crate) fn overwrite<P: AsRef<Path>, C: AsRef<[u8]>>(
    path: P,
    contents: C,
) -> anyhow::Result<()> {
    let path = path.as_ref();

    let parent = path.parent().context("path has no parent")?;

    fs::create_dir_all(parent).context("failed to create parent path")?;

    let mut temp_filename = ".DELETE_ME_".to_owned();
    rand::distributions::Alphanumeric.append_string(
        &mut rand::thread_rng(),
        &mut temp_filename,
        20,
    );
    let temp_path = parent.join(temp_filename);

    fs::write(&*temp_path, contents).context("failed to write to temporary file")?;

    if let Err(e) = fs::rename(&*temp_path, &*path) {
        drop(fs::remove_file(temp_path));
        return Err(e).context("failed to overwrite with new file")?;
    }

    Ok(())
}

use anyhow::Context as _;
use rand::distributions::DistString;
use std::fs;
use std::path::Path;
