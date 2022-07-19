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
    clippy::large_enum_variant,
    clippy::struct_excessive_bools,
    clippy::needless_pass_by_value,
    clippy::single_char_pattern,
    clippy::match_bool
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
    Errored(Errored),
}

impl Mode<'_> {
    fn entry_content(&self, line: usize) -> &str {
        match &self.state {
            State::Initialized(initialized) => initialized.entry_content(line),
            State::Errored(_) => panic!("this mode has no entries"),
        }
    }
}

impl<'rofi> rofi_mode::Mode<'rofi> for Mode<'rofi> {
    const NAME: &'static str = "bw\0";
    fn init(mut api: rofi_mode::Api<'rofi>) -> Result<Self, ()> {
        let mut pipe = None;

        let res = (|| {
            let pipe = pipe.insert(get_pipe()?);
            let handshake = ipc::handshake::read(pipe)?;
            std::fs::write("/home/sabrina/data.json", &handshake.data).unwrap();
            let data =
                serde_json::from_slice(&*handshake.data).context("failed to read vault data")?;
            Initialized::new(&handshake.master_key, data)
        })();

        let state = res
            .map_err(Errored::new)
            .map_or_else(State::Errored, State::Initialized);

        api.set_display_name(match &state {
            State::Initialized(_) => Initialized::DISPLAY_NAME,
            State::Errored(_) => Errored::DISPLAY_NAME,
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

    fn react(
        &mut self,
        event: rofi_mode::Event,
        input: &mut rofi_mode::String,
    ) -> rofi_mode::Action {
        match event {
            rofi_mode::Event::Cancel { selected: _ } => {
                request(&mut self.pipe, ipc::MenuRequest::Exit);
                rofi_mode::Action::Exit
            }
            rofi_mode::Event::Ok { alt: _, selected } => match &mut self.state {
                State::Initialized(initialized) => {
                    request(&mut self.pipe, initialized.ok(selected));
                    rofi_mode::Action::Exit
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
                let keybind = match rofi_bw_common::KEYBINDS.get(usize::from(number)) {
                    Some(keybind) => keybind,
                    None => return rofi_mode::Action::Reload,
                };
                request(&mut self.pipe, keybind.action);
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
            for keybind in rofi_bw_common::KEYBINDS {
                if !message.is_empty() {
                    message.push_str(" | ");
                }

                message.push_str("<b>");
                message.push_str(keybind.combination);
                message.push_str("</b>: ");
                message.push_str(keybind.description);
            }
        }

        match &self.state {
            State::Initialized(_) => {}
            State::Errored(errored) => {
                if !message.is_empty() {
                    message.push_str("\n\n");
                }
                message.push_str(errored.message());
            }
        }

        message
    }
}

use get_pipe::get_pipe;
mod get_pipe {
    pub(crate) fn get_pipe() -> anyhow::Result<UnixStream> {
        inner().context(
            "\
            failed to read pipe fd from environment;\
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

fn request(pipe: &mut Option<BufWriter<UnixStream>>, request: ipc::MenuRequest<&str>) {
    if let Some(pipe) = pipe {
        let res = (|| {
            request.write(pipe)?;
            pipe.flush().context("failed to flush pipe")?;
            anyhow::Ok(())
        })();
        if let Err(e) = res {
            eprintln!("Error: {:?}", e);
        }
    }
}

use initialized::Initialized;
mod initialized;

use errored::Errored;
mod errored;

mod data;

use cipher_string::CipherString;
mod cipher_string;

use symmetric_key::SymmetricKey;
mod symmetric_key;

use base64_decode_array::base64_decode_array;
mod base64_decode_array;

use anyhow::Context as _;
use rofi_bw_common::ipc;
use std::io::BufWriter;
use std::io::Write;
use std::os::unix::net::UnixStream;
