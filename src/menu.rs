// TODO: refactor to a single function?
pub(crate) struct Menu {
    rofi: process::Child,
    pipe: BufReader<UnixStream>,
}

impl Menu {
    pub(crate) fn open(
        lib_dir: &OsStr,
        master_key: &MasterKey,
        data: &str,
    ) -> anyhow::Result<Self> {
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
        unsafe {
            // Turn off close-on-exec for the pipe fd.
            rofi.pre_exec(move || {
                let previous = syscall_result(libc::fcntl(pipe_fd, libc::F_GETFD))?;
                let new = previous & !libc::FD_CLOEXEC;
                if new != previous {
                    syscall_result(libc::fcntl(pipe_fd, libc::F_SETFD, new))?;
                }
                Ok(())
            });
        }

        let rofi = rofi.spawn().context("failed to spawn rofi")?;

        drop(child_stream);

        ipc::handshake::write(&mut parent_stream, master_key, data.as_bytes())?;

        Ok(Self {
            rofi,
            pipe: BufReader::new(parent_stream),
        })
    }

    pub(crate) fn read_request(
        &mut self,
    ) -> Result<ipc::MenuRequest<Box<str>>, ipc::menu_request::ReadError<io::Error>> {
        ipc::MenuRequest::read(&mut self.pipe)
    }

    pub(crate) fn wait(mut self) -> anyhow::Result<()> {
        drop(self.pipe);
        let status = self.rofi.wait().context("failed to wait on rofi")?;
        anyhow::ensure!(status.success(), "rofi failed with {status}");
        Ok(())
    }
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
use std::os::unix::net::UnixStream;
use std::os::unix::prelude::AsRawFd;
use std::os::unix::prelude::CommandExt;
use std::process;
