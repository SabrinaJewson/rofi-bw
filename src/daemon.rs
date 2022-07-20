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
}

impl Daemon {
    pub(crate) fn bind(runtime_dir: &Path) -> anyhow::Result<Self> {
        let socket_path = runtime_dir.join(SOCKET_FILE_NAME);

        drop(fs::create_dir_all(runtime_dir));
        drop(fs::remove_file(&*socket_path));

        let socket = UnixDatagram::bind(&*socket_path)
            .with_context(|| format!("failed to bind to socket at {}", socket_path.display()))?;

        Ok(Self { socket })
    }

    pub(crate) fn wait(&mut self) {
        let mut buf = [0; 1];

        loop {
            // TODO: Timeout to auto-lock
            if let Err(e) = self.socket.recv(&mut buf) {
                eprintln!(
                    "Warning: {:?}",
                    anyhow::Error::new(e).context("failed to recv")
                );
                thread::sleep(std::time::Duration::from_secs(2));
                return;
            }

            match buf {
                [commands::SHOW] => break,
                [command] => eprintln!("Warning: received unknown command {command}"),
            }
        }
    }
}

const SOCKET_FILE_NAME: &str = "rofi-bw-session";

mod commands {
    /// Show the password chooser menu
    pub(crate) const SHOW: u8 = 0;
}

use anyhow::Context as _;
use std::fs;
use std::io;
use std::os::unix::net::UnixDatagram;
use std::path::Path;
use std::thread;