pub(crate) fn run(
    lib_dir: &OsStr,
    handshake: &ipc::Handshake<&MasterKey, &[u8]>,
    rofi_options: &config::RofiOptions,
    display: &str,
) -> anyhow::Result<ipc::MenuRequest<String>> {
    let (parent_stream, child_stream) =
        UnixStream::pair().context("failed to create IPC channel")?;

    let mut rofi = process::Command::new(&*rofi_options.binary);
    rofi.env("ROFI_PLUGIN_PATH", lib_dir);
    rofi.arg("-modi").arg("bw");
    rofi.arg("-show").arg("bw");

    apply_options(&mut rofi, rofi_options);
    rofi.arg("-display").arg(display);

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

    // Capture IPC errors, because status code errors should take precedence
    // (and we also don't want a zombie process).
    let ipc_result: anyhow::Result<_> = (|| {
        let mut pipe = BufWriter::new(parent_stream);
        ipc::handshake::write(&mut pipe, handshake)?;
        let pipe = pipe.into_inner()?;

        let mut pipe = BufReader::new(pipe);
        Ok(ipc::menu_request::read(&mut pipe)?)
    })();

    let status = rofi.wait().context("failed to wait on rofi")?;
    anyhow::ensure!(status.success(), "rofi failed with {status}");

    ipc_result
}

fn apply_options(rofi: &mut process::Command, rofi_options: &config::RofiOptions) {
    if rofi_options.threads != 0 {
        rofi.arg("-threads")
            .arg(itoa::Buffer::new().format(rofi_options.threads));
    }

    if rofi_options.case_sensitive {
        rofi.arg("-case-sensitive");
    }

    if let Some(cycle) = rofi_options.cycle {
        rofi.arg("-cycle").arg(match cycle {
            false => "false",
            true => "true",
        });
    }

    if let Some(config) = &rofi_options.config {
        rofi.arg("-config").arg(config);
    }

    if let Some(scroll_method) = rofi_options.scroll_method {
        rofi.arg("-scroll-method")
            .arg(itoa::Buffer::new().format(scroll_method as u32));
    }

    if rofi_options.normalize_match {
        rofi.arg("-normalize-match");
    }

    if !rofi_options.lazy_grab {
        rofi.arg("-no-lazy-grab");
    }

    if rofi_options.normal_window {
        rofi.arg("-normal-window");
    }

    if let Some(matching_method) = rofi_options.matching {
        rofi.arg("-matching").arg(match matching_method {
            config::Matching::Normal => "normal",
            config::Matching::Regex => "regex",
            config::Matching::Glob => "glob",
            config::Matching::Fuzzy => "fuzzy",
            config::Matching::Prefix => "prefix",
        });
    }

    if let Some(matching_negate_char) = &rofi_options.matching_negate_char {
        rofi.arg("-matching-negate-char")
            .arg(&*matching_negate_char);
    }

    if let Some(theme) = &rofi_options.theme {
        rofi.arg("-theme").arg(theme);
    }

    if !rofi_options.theme_str.is_empty() {
        rofi.arg("-theme-str").arg(&*rofi_options.theme_str);
    }

    if !rofi_options.click_to_exit {
        rofi.arg("-no-click-to-exit");
    }
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

use crate::config;
use anyhow::Context as _;
use rofi_bw_common::ipc;
use rofi_bw_common::MasterKey;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io;
use std::io::BufReader;
use std::io::BufWriter;
use std::os::raw::c_int;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process;
