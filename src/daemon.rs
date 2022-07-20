pub(crate) fn invoke(runtime_dir: &Path) -> anyhow::Result<bool> {
    invoke_inner(runtime_dir).context("failed to invoke daemon")
}

fn invoke_inner(runtime_dir: &Path) -> anyhow::Result<bool> {
    let socket_path = runtime_dir.join(SOCKET_FILE_NAME);

    let socket = UnixDatagram::unbound().context("failed to create client socket")?;
    match socket.send_to(&[commands::SHOW], socket_path) {
        Ok(_) => Ok(true),
        Err(e)
            if [io::ErrorKind::NotFound, io::ErrorKind::ConnectionRefused].contains(&e.kind()) =>
        {
            Ok(false)
        }
        Err(e) => Err(anyhow::Error::new(e).context("failed to send to daemon")),
    }
}

pub(crate) struct Daemon {
    socket: UnixDatagram,
    should_wait: bool,
}

impl Daemon {
    pub(crate) fn bind(runtime_dir: &Path, auto_lock: AutoLock) -> anyhow::Result<Self> {
        let socket_path = runtime_dir.join(SOCKET_FILE_NAME);

        drop(fs::create_dir_all(runtime_dir));
        drop(fs::remove_file(&*socket_path));

        let mut socket = UnixDatagram::bind(&*socket_path)
            .with_context(|| format!("failed to bind to socket at {}", socket_path.display()))?;

        let should_wait = match auto_lock {
            AutoLock::After(Duration::ZERO) => false,
            AutoLock::After(timeout) => {
                socket
                    .set_read_timeout(Some(timeout))
                    .context("failed to set socket timeout")?;
                true
            }
            AutoLock::Never => true,
        };

        Ok(Self {
            socket,
            should_wait,
        })
    }

    pub(crate) fn wait(&mut self) -> Event {
        if !self.should_wait {
            return Event::Timeout;
        }

        let mut buf = [0; 1];

        loop {
            if let Err(e) = self.socket.recv(&mut buf) {
                if e.kind() == io::ErrorKind::WouldBlock {
                    break Event::Timeout;
                }
                eprintln!(
                    "Warning: {:?}",
                    anyhow::Error::new(e).context("failed to recv")
                );
                thread::sleep(std::time::Duration::from_secs(2));
                continue;
            }

            match buf {
                [commands::SHOW] => break Event::ShowMenu,
                [command] => eprintln!("Warning: received unknown command {command}"),
            }
        }
    }
}

pub(crate) enum Event {
    ShowMenu,
    Timeout,
}

const SOCKET_FILE_NAME: &str = "rofi-bw-session";

mod commands {
    /// Show the password chooser menu
    pub(crate) const SHOW: u8 = 0;
}

use crate::config::AutoLock;
use anyhow::Context as _;
use std::fs;
use std::io;
use std::os::unix::net::UnixDatagram;
use std::path::Path;
use std::thread;
use std::time::Duration;
