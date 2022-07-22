#[derive(bincode::Encode, bincode::BorrowDecode)]
pub(crate) enum Command<'display> {
    ShowMenu {
        /// The value of the `$DISPLAY` environment variable.
        display: &'display str,
    },
    Quit,
}

pub(crate) fn invoke(runtime_dir: &Path, command: &Command<'_>) -> anyhow::Result<bool> {
    invoke_inner(runtime_dir, command).context("failed to invoke daemon")
}

fn invoke_inner(runtime_dir: &Path, command: &Command<'_>) -> anyhow::Result<bool> {
    let socket_path = runtime_dir.join(SOCKET_FILE_NAME);

    let socket = UnixDatagram::unbound().context("failed to create client socket")?;

    let command = bincode::encode_to_vec(command, bincode_config()).unwrap();

    match socket.send_to(&*command, socket_path) {
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
    buf: Box<[u8]>,
    should_wait: bool,
}

impl Daemon {
    pub(crate) fn bind(runtime_dir: &Path, auto_lock: AutoLock) -> anyhow::Result<Self> {
        let socket_path = runtime_dir.join(SOCKET_FILE_NAME);

        drop(fs::create_dir_all(runtime_dir));
        drop(fs::remove_file(&*socket_path));

        let socket = UnixDatagram::bind(&*socket_path)
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
            buf: vec![0; 8192].into_boxed_slice(),
            should_wait,
        })
    }

    pub(crate) fn wait(&mut self) -> Command<'_> {
        if !self.should_wait {
            return Command::Quit;
        }

        let mut buf = &mut *self.buf;

        loop {
            let bytes = match self.socket.recv(buf) {
                Ok(bytes) => bytes,
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        break Command::Quit;
                    }
                    eprintln!(
                        "Warning: {:?}",
                        anyhow::Error::new(e).context("failed to recv")
                    );
                    thread::sleep(std::time::Duration::from_secs(2));
                    continue;
                }
            };

            polonius!(|buf| -> Command<'polonius> {
                match bincode::decode_from_slice(&buf[..bytes], bincode_config()) {
                    Ok((command, _)) => polonius_return!(command),
                    Err(e) => {
                        eprintln!(
                            "Warning: {:?}",
                            anyhow!(e).context("failed to deserialize command")
                        );
                    }
                }
            });
        }
    }
}

const SOCKET_FILE_NAME: &str = "rofi-bw-session";

fn bincode_config() -> impl bincode::config::Config {
    bincode::config::standard()
}

use crate::config::AutoLock;
use anyhow::anyhow;
use anyhow::Context as _;
use polonius_the_crab::polonius;
use polonius_the_crab::polonius_return;
use std::fs;
use std::io;
use std::os::unix::net::UnixDatagram;
use std::path::Path;
use std::thread;
use std::time::Duration;
