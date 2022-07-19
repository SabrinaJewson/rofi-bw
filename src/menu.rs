pub(crate) fn run(
    lib_dir: &OsStr,
    master_key: &MasterKey,
    data: &str,
) -> anyhow::Result<ipc::MenuRequest<Box<str>>> {
    let (mut parent_stream, child_stream) =
        UnixStream::pair().context("failed to create IPC channel")?;

    let mut rofi = process::Command::new("rofi");
    rofi.env("ROFI_PLUGIN_PATH", lib_dir);
    rofi.arg("-modi").arg("bw");
    rofi.arg("-show").arg("bw");

    let mut arg_name_buf = String::new();
    for (i, keybind) in rofi_bw_common::KEYBINDS.iter().enumerate() {
        arg_name_buf.clear();
        write!(arg_name_buf, "-kb-custom-{}", i + 1).unwrap();
        rofi.arg(&*arg_name_buf).arg(keybind.combination);
    }

    let pipe_fd = child_stream.as_raw_fd();
    rofi.env(ipc::PIPE_FD_ENV_VAR, itoa::Buffer::new().format(pipe_fd));
    unsafe { rofi.pre_exec(move || unset_cloexec(pipe_fd)) };

    let mut rofi = rofi.spawn().context("failed to spawn rofi")?;

    drop(child_stream);

    ipc::handshake::write(&mut parent_stream, master_key, data.as_bytes())?;

    let mut pipe = BufReader::new(parent_stream);
    let res = ipc::MenuRequest::read(&mut pipe);

    drop(pipe);

    let status = rofi.wait().context("failed to wait on rofi")?;
    anyhow::ensure!(status.success(), "rofi failed with {status}");

    Ok(res?)
}

fn unset_cloexec(fd: RawFd) -> io::Result<()> {
    let previous = syscall_result(unsafe { libc::fcntl(fd, libc::F_GETFD) })?;
    let new = previous & !libc::FD_CLOEXEC;
    if new != previous {
        syscall_result(unsafe { libc::fcntl(fd, libc::F_SETFD, new) })?;
    }
    Ok(())
}

fn syscall_result(res: c_int) -> io::Result<c_int> {
    if res == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(res)
    }
}

use anyhow::Context as _;
use rofi_bw_common::ipc;
use rofi_bw_common::MasterKey;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io;
use std::io::BufReader;
use std::os::raw::c_int;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process;
