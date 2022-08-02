#[derive(bincode::Encode, bincode::Decode)]
pub(crate) enum Request {
    ShowMenu(ShowMenu),
    Quit,
}

#[derive(Clone, bincode::Encode, bincode::Decode)]
pub(crate) struct ShowMenu {
    /// The value of the `$DISPLAY` environment variable.
    pub(crate) display: String,

    /// The initial filter to start Rofi with.
    pub(crate) filter: String,

    /// The view to display in `rofi-bw`
    pub(crate) view: ipc::View,
}

#[derive(bincode::Encode, bincode::Decode)]
enum Response {
    Ok,
    Busy,
}

pub(crate) fn invoke(runtime_dir: &fs::Path, request: &Request) -> anyhow::Result<bool> {
    invoke_inner(runtime_dir, request).context("failed to invoke daemon")
}

fn invoke_inner(runtime_dir: &fs::Path, request: &Request) -> anyhow::Result<bool> {
    let socket_path = runtime_dir.join(SOCKET_FILE_NAME);

    let acceptable_errors = [io::ErrorKind::NotFound, io::ErrorKind::ConnectionRefused];
    let mut socket = match UnixStream::connect(socket_path) {
        Ok(socket) => socket,
        Err(e) if acceptable_errors.contains(&e.kind()) => return Ok(false),
        Err(e) => return Err(e).context("failed to connect to client socket"),
    };

    let request = bincode::encode_to_vec(request, bincode_config()).unwrap();

    socket
        .write_all(&*request)
        .context("failed to send to daemon")?;

    socket
        .shutdown(net::Shutdown::Write)
        .context("failed to shutdown write end of pipe")?;

    let mut response = request;
    response.clear();

    socket
        .read_to_end(&mut response)
        .context("failed to read from daemon")?;

    let (response, _) = bincode::decode_from_slice(&*response, bincode_config())
        .context("failed to decode daemon response")?;

    match response {
        Response::Ok => {}
        Response::Busy => anyhow::bail!("menu is already open"),
    }

    Ok(true)
}

pub(crate) struct Daemon {
    shared: Arc<Shared>,
    auto_lock: AutoLock,
}

struct Shared {
    state: Mutex<State>,
    transfer_start: Condvar,
}

enum State {
    ShowingMenu,
    Waiting,
    Transferring(Request),
}

impl Daemon {
    pub(crate) fn bind(runtime_dir: &fs::Path, auto_lock: AutoLock) -> anyhow::Result<Self> {
        let socket_path = runtime_dir.join(SOCKET_FILE_NAME);

        drop(fs::create_dir_all(runtime_dir));
        drop(std::fs::remove_file(&*socket_path));

        let listener = UnixListener::bind(&*socket_path)
            .with_context(|| format!("failed to bind to socket at {}", socket_path.display()))?;

        let shared = Arc::new(Shared {
            state: Mutex::new(State::ShowingMenu),
            transfer_start: Condvar::new(),
        });

        thread::Builder::new()
            .spawn({
                let shared = shared.clone();
                move || background_thread(shared, listener)
            })
            .context("failed to spawn listener thread")?;

        Ok(Self { shared, auto_lock })
    }

    pub(crate) fn wait(&mut self) -> Request {
        if self.auto_lock == AutoLock::After(Duration::ZERO) {
            return Request::Quit;
        }

        let mut state = self.shared.state.lock().unwrap();
        match *state {
            State::Waiting | State::Transferring(_) => unreachable!(),
            State::ShowingMenu => {}
        }
        *state = State::Waiting;

        let condition = |state: &mut State| match *state {
            State::ShowingMenu => unreachable!(),
            State::Waiting => true,
            State::Transferring(_) => false,
        };

        match self.auto_lock {
            AutoLock::Never => {
                state = self
                    .shared
                    .transfer_start
                    .wait_while(state, condition)
                    .unwrap();
            }
            AutoLock::After(timeout) => {
                let (new_state, res) = self
                    .shared
                    .transfer_start
                    .wait_timeout_while(state, timeout, condition)
                    .unwrap();
                state = new_state;
                if res.timed_out() {
                    return Request::Quit;
                }
            }
        }

        match mem::replace(&mut *state, State::ShowingMenu) {
            State::Transferring(request) => request,
            _ => unreachable!(),
        }
    }
}

fn background_thread(shared: Arc<Shared>, listener: UnixListener) -> ! {
    loop {
        let connection_errors = [
            io::ErrorKind::ConnectionRefused,
            io::ErrorKind::ConnectionAborted,
            io::ErrorKind::ConnectionReset,
        ];

        let connection = match listener.accept() {
            Ok((connection, _)) => connection,
            Err(e) if connection_errors.contains(&e.kind()) => continue,
            Err(e) => {
                let e = anyhow!(e).context("failed to accept connection");
                eprintln!("Warning: {e:?}");
                thread::sleep(Duration::from_secs(2));
                continue;
            }
        };

        let shared = shared.clone();
        drop(thread::Builder::new().spawn(|| handle_connection(shared, connection)));
    }
}

fn handle_connection(shared: Arc<Shared>, connection: UnixStream) {
    handle_connection_inner(shared, connection);
}

fn handle_connection_inner(shared: Arc<Shared>, mut connection: UnixStream) -> Option<()> {
    let mut buf = Vec::with_capacity(64);
    connection.read_to_end(&mut buf).ok()?;
    let (request, _) = bincode::decode_from_slice(&*buf, bincode_config()).ok()?;

    let response = {
        let mut state = shared.state.lock().unwrap();
        match *state {
            State::Waiting => {
                *state = State::Transferring(request);
                shared.transfer_start.notify_one();
                Response::Ok
            }
            State::ShowingMenu | State::Transferring(_) => Response::Busy,
        }
    };

    buf.clear();
    bincode::encode_into_std_write(response, &mut buf, bincode_config()).unwrap();
    connection.write_all(&*buf).ok()?;

    Some(())
}

const SOCKET_FILE_NAME: &str = "rofi-bw-session";

fn bincode_config() -> impl bincode::config::Config {
    bincode::config::standard()
}

use crate::config::AutoLock;
use anyhow::anyhow;
use anyhow::Context as _;
use rofi_bw_common::ipc;
use rofi_bw_util::fs;
use std::io;
use std::io::Read;
use std::io::Write;
use std::mem;
use std::net;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
