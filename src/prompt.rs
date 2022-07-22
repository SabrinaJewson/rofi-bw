#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Visibility {
    Shown,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Entered(usize),
    Cancelled,
}

pub(crate) fn prompt(
    msg: &str,
    visibility: Visibility,
    buf: &mut String,
) -> anyhow::Result<Outcome> {
    let mut rofi = process::Command::new("rofi");
    rofi.arg("-dmenu");
    rofi.stdin(process::Stdio::null())
        .stdout(process::Stdio::piped());
    rofi.arg("-p").arg(msg);
    if visibility == Visibility::Hidden {
        rofi.arg("-password");
    }
    let mut rofi = rofi.spawn().context("failed to spawn rofi")?;

    let mut stdout = rofi.stdout.take().unwrap();
    let mut bytes_read = stdout
        .read_to_string(buf)
        .context("failed to read Rofi's output")?;

    let status = rofi.wait().context("failed to wait on Rofi")?;
    if !status.success() {
        return Ok(Outcome::Cancelled);
    }

    if bytes_read > 0 && buf.ends_with("\n") {
        buf.pop();
        bytes_read -= 1;
    }

    Ok(Outcome::Entered(bytes_read))
}

use anyhow::Context as _;
use std::io::Read;
use std::process;
