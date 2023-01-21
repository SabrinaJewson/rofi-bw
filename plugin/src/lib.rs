#![warn(
    clippy::pedantic,
    noop_method_call,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_op_in_unsafe_fn,
    unused_lifetimes,
    unused_qualifications
)]
#![allow(
    clippy::single_match_else,
    clippy::match_wildcard_for_single_variants,
    clippy::struct_excessive_bools,
    clippy::needless_pass_by_value,
    clippy::single_char_pattern,
    clippy::match_bool,
    clippy::items_after_statements,
    clippy::single_char_add_str
)]

rofi_mode::export_mode!(Mode<'_>);

struct Mode<'rofi> {
    #[allow(dead_code)]
    api: rofi_mode::Api<'rofi>,
    pipe: Option<BufWriter<UnixStream>>,
    state: State,
}

enum State {
    Initialized(Initialized),
    Errored(String),
}

impl Mode<'_> {
    fn entry_content(&self, line: usize) -> &str {
        match &self.state {
            State::Initialized(initialized) => initialized.entry_content(line),
            State::Errored(_) => panic!("this mode has no entries"),
        }
    }

    fn initialized_mut(&mut self) -> Option<&mut Initialized> {
        match &mut self.state {
            State::Initialized(initialized) => Some(initialized),
            _ => None,
        }
    }

    fn menu_state(&self, input: &str) -> ipc::menu_request::MenuState {
        ipc::menu_request::MenuState {
            filter: input.to_string(),
            history: match &self.state {
                State::Initialized(initialized) => initialized.ipc_state(),
                State::Errored(_) => History::default(),
            },
        }
    }
}

impl<'rofi> rofi_mode::Mode<'rofi> for Mode<'rofi> {
    const NAME: &'static str = "bw\0";
    fn init(mut api: rofi_mode::Api<'rofi>) -> Result<Self, ()> {
        let mut pipe = None;

        let res = (|| {
            let pipe = BufReader::new(pipe.insert(get_pipe()?));
            let ipc::Handshake {
                master_key,
                data,
                history,
            } = ipc::handshake::read(pipe)?;
            let data = serde_json::from_slice(&data).context("failed to read vault data")?;
            Initialized::new(&master_key, data, history)
        })();

        let state = res
            .map_err(error_status)
            .map_or_else(State::Errored, State::Initialized);

        api.set_display_name(match &state {
            State::Initialized(_) => Initialized::DISPLAY_NAME,
            State::Errored(_) => "Error",
        });

        let pipe = pipe.map(BufWriter::new);

        Ok(Self { api, pipe, state })
    }

    fn entries(&mut self) -> usize {
        match &self.state {
            State::Initialized(initialized) => initialized.entries(),
            State::Errored(_) => 0,
        }
    }

    fn entry_content(&self, line: usize) -> rofi_mode::String {
        self.entry_content(line).into()
    }

    fn entry_icon(&mut self, line: usize, height: u32) -> Option<cairo::Surface> {
        match &mut self.state {
            State::Initialized(initialized) => initialized.entry_icon(line, height),
            State::Errored(_) => panic!("this mode has no entries"),
        }
    }

    fn react(
        &mut self,
        event: rofi_mode::Event,
        input: &mut rofi_mode::String,
    ) -> rofi_mode::Action {
        match event {
            rofi_mode::Event::Cancel { selected: _ } => {
                let menu_state = self.menu_state(input);
                send_request(&mut self.pipe, &ipc::MenuRequest::Exit { menu_state });
                rofi_mode::Action::Exit
            }
            rofi_mode::Event::Ok { alt, selected } => match &mut self.state {
                State::Initialized(initialized) => {
                    let request = if alt {
                        initialized.ok_alt(selected, input);
                        None
                    } else {
                        initialized.ok(selected, input)
                    };

                    match request {
                        Some(request) => {
                            send_request(&mut self.pipe, &request);
                            rofi_mode::Action::Exit
                        }
                        None => rofi_mode::Action::Reload,
                    }
                }
                State::Errored(_) => panic!("this mode has no entries"),
            },
            rofi_mode::Event::Complete {
                selected: Some(selected),
            } => {
                input.clear();
                input.push_str(self.entry_content(selected));
                rofi_mode::Action::Reload
            }
            rofi_mode::Event::CustomCommand {
                number,
                selected: _,
            } => {
                let keybind = match MENU_KEYBINDS.get(usize::from(number)) {
                    Some(keybind) => keybind,
                    None => return rofi_mode::Action::Reload,
                };
                let request = match keybind.action {
                    menu_keybinds::Action::ShowList(list) => {
                        if let Some(initialized) = self.initialized_mut() {
                            initialized.show(list);
                        }
                        return rofi_mode::Action::Reload;
                    }
                    menu_keybinds::Action::Parent => {
                        if let Some(initialized) = self.initialized_mut() {
                            initialized.parent();
                        }
                        return rofi_mode::Action::Reload;
                    }
                    menu_keybinds::Action::Sync => ipc::MenuRequest::Sync {
                        menu_state: self.menu_state(input),
                    },
                    menu_keybinds::Action::Lock => ipc::MenuRequest::Lock,
                    menu_keybinds::Action::LogOut => ipc::MenuRequest::LogOut,
                    menu_keybinds::Action::Navigate(navigate) => {
                        if let Some(initialized) = self.initialized_mut() {
                            initialized.navigate(navigate);
                        }
                        return rofi_mode::Action::Reload;
                    }
                };
                send_request(&mut self.pipe, &request);
                rofi_mode::Action::Exit
            }
            rofi_mode::Event::CustomInput {
                alt: _,
                selected: _,
            }
            | rofi_mode::Event::Complete { selected: None }
            | rofi_mode::Event::DeleteEntry { selected: _ } => rofi_mode::Action::Reload,
        }
    }

    fn matches(&self, line: usize, matcher: rofi_mode::Matcher<'_>) -> bool {
        matcher.matches(self.entry_content(line))
    }

    fn message(&mut self) -> rofi_mode::String {
        let mut message = rofi_mode::String::new();

        if self.pipe.is_some() {
            let history = self.initialized_mut().map(|i| i.history());
            menu_keybinds::keybinds_ui(history, |row| {
                writeln!(message, "{}", keybind::HelpMarkup(row)).unwrap();
            });
        }

        writeln!(message).unwrap();

        match &self.state {
            State::Initialized(initialized) => initialized.status(&mut message),
            State::Errored(errored) => message.push_str(errored),
        }

        while message.ends_with("\n") {
            message.pop();
        }

        message
    }
}

use get_pipe::get_pipe;
mod get_pipe {
    pub(crate) fn get_pipe() -> anyhow::Result<UnixStream> {
        inner().context(
            "\
            failed to read pipe fd from environment; \
            are you running inside rofi-bw?\
        ",
        )
    }

    fn inner() -> anyhow::Result<UnixStream> {
        static TAKEN: AtomicBool = AtomicBool::new(false);
        TAKEN
            .compare_exchange(
                false,
                true,
                atomic::Ordering::Relaxed,
                atomic::Ordering::Relaxed,
            )
            .expect("Called `get_pipe()` multiple times");

        let pipe_fd =
            env::var(ipc::PIPE_FD_ENV_VAR).context("failed to get pipe fd environment variable")?;

        let pipe_fd = pipe_fd
            .parse::<RawFd>()
            .context("pipe fd env var is not a number")?;

        Ok(unsafe { UnixStream::from_raw_fd(pipe_fd) })
    }

    use anyhow::Context as _;
    use rofi_bw_common::ipc;
    use std::env;
    use std::os::unix::io::FromRawFd;
    use std::os::unix::io::RawFd;
    use std::os::unix::net::UnixStream;
    use std::sync::atomic;
    use std::sync::atomic::AtomicBool;
}

fn send_request(pipe: &mut Option<BufWriter<UnixStream>>, request: &MenuRequest) {
    if let Some(pipe) = pipe {
        let res = (|| {
            ipc::menu_request::write(&mut *pipe, request)?;
            pipe.flush().context("failed to flush pipe")?;
            anyhow::Ok(())
        })();
        if let Err(e) = res {
            eprintln!("Error: {e:?}");
        }
    }
}

use error_status::error_status;
mod error_status {
    pub(crate) fn error_status(error: anyhow::Error) -> String {
        let escaped = glib::markup_escape_text(&format!("{error:?}"));
        format!("\n<span foreground='red'>Error:</span> {escaped}")
    }

    use rofi_mode::pango::glib;
}

use initialized::Initialized;
mod initialized;

use icons::Icon;
use icons::Icons;
mod icons;

mod data;

use cipher_string::CipherString;
mod cipher_string;

use symmetric_key::SymmetricKey;
mod symmetric_key;

use base64_decode_array::base64_decode_array;
mod base64_decode_array;

use disk_cache::DiskCache;
mod disk_cache;

use cairo_image_data::CairoImageData;
mod cairo_image_data;

use poll_future_once::poll_future_once;
mod poll_future_once;

use sync_wrapper::SyncWrapper;
mod sync_wrapper;

use anyhow::Context as _;
use rofi_bw_common::ipc;
use rofi_bw_common::ipc::MenuRequest;
use rofi_bw_common::keybind;
use rofi_bw_common::menu_keybinds;
use rofi_bw_common::MENU_KEYBINDS;
use rofi_bw_util::History;
use rofi_mode::cairo;
use std::fmt::Write as _;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::Write;
use std::os::unix::net::UnixStream;
