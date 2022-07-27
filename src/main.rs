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
    clippy::single_char_pattern,
    clippy::struct_excessive_bools,
    clippy::items_after_statements,
    clippy::match_bool,
    clippy::match_wildcard_for_single_variants,
    clippy::needless_pass_by_value
)]

fn main() -> process::ExitCode {
    if let Err(e) = try_main(Args::parse()) {
        report_error(e.as_ref());
        return process::ExitCode::FAILURE;
    }
    process::ExitCode::SUCCESS
}

/// Rofi interface to Bitwarden.
#[derive(clap::Parser)]
struct Args {
    /// The initial filter to use in Rofi
    #[clap(short, long, default_value = "")]
    filter: String,

    /// Path to the config file; defaults to `$XDG_CONFIG_DIR/rofi-bw/config.toml`.
    ///
    /// Note that this will not be taken into account if an instance of rofi-bw is already running.
    #[clap(short, long)]
    config_file: Option<fs::PathBuf>,
}

fn try_main(
    Args {
        filter,
        config_file,
    }: Args,
) -> anyhow::Result<()> {
    let project_dirs = ProjectDirs::from("", "", "rofi-bw").context("no home directory")?;

    let runtime_dir = project_dirs
        .runtime_dir()
        .context("failed to locate runtime directory")?;

    let display = env::var("DISPLAY").context("failed to read `$DISPLAY` env var")?;

    let request = daemon::Request::ShowMenu(daemon::ShowMenu { display, filter });
    if daemon::invoke(runtime_dir, &request)? {
        return Ok(());
    }
    let mut request = match request {
        daemon::Request::ShowMenu(request) => request,
        _ => unreachable!(),
    };

    // Having failed to invoke an existing daemon, we must now become the daemon.

    let config_path = config_file.unwrap_or_else(|| project_dirs.config_dir().join("config.toml"));
    let Config {
        auto_lock,
        copy_notification,
        rofi_options,
        client_id,
        device_type,
        device_name,
    } = config::load(&*config_path)?;

    let mut daemon = Daemon::bind(runtime_dir, auto_lock)?;

    let http = ureq::agent();

    let mut session_manager =
        SessionManager::new(&http, &project_dirs, &*client_id, device_type, device_name)?;

    let mut menu_opts = MenuOpts {
        lib_dir: env::var_os("ROFI_BW_LIB_DIR")
            .unwrap_or_else(|| "/usr/lib/rofi-bw:/usr/local/lib/rofi-bw".into()),
        rofi_options,
        copy_notification,
        clipboard: Clipboard::new().context("failed to open clipboard")?,
    };

    while let Some(mut session) = session_manager.start_session()? {
        loop {
            let after_menu = show_menu(
                &mut session,
                &mut menu_opts,
                &*request.display,
                &*request.filter,
            );
            match after_menu {
                AfterMenu::ShowMenuAgain {
                    after_unlock,
                    menu_state,
                } => {
                    request.filter = menu_state.filter;
                    if after_unlock {
                        break;
                    }
                }
                AfterMenu::ContinueServing => match daemon.wait() {
                    daemon::Request::ShowMenu(new) => request = new,
                    daemon::Request::Quit => return Ok(()),
                },
                AfterMenu::LogOut => {
                    request.filter.clear();
                    // TODO: check errors
                    session_manager.log_out()?;
                    break;
                }
                AfterMenu::StopServing => return Ok(()),
            }
        }
    }

    Ok(())
}

struct SessionManager<'http, 'dirs, 'client_id> {
    http: &'http ureq::Agent,
    project_dirs: &'dirs ProjectDirs,
    data: Data,
    client_id: &'client_id str,
    device_type: auth::DeviceType,
    device_name: String,
}

impl<'http, 'dirs, 'client_id> SessionManager<'http, 'dirs, 'client_id> {
    fn new(
        http: &'http ureq::Agent,
        project_dirs: &'dirs ProjectDirs,
        client_id: &'client_id str,
        device_type: auth::DeviceType,
        device_name: String,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            http,
            project_dirs,
            data: Data::load(project_dirs.data_dir())?,
            client_id,
            device_type,
            device_name,
        })
    }

    fn start_session(&mut self) -> anyhow::Result<Option<Session<'http, 'client_id>>> {
        loop {
            if self.data.email.is_none() {
                self.data.email = Some(match ask_email()? {
                    Some(email) => email,
                    None => return Ok(None),
                });
                self.data.store()?;
            }
            let email = self.data.email.as_ref().unwrap();

            let mut again = false;
            loop {
                let keybinds = &[Keybind {
                    combination: "Control+o",
                    action: (),
                    description: "Log out",
                }];
                let master_password = match ask_master_password(again, "", keybinds)? {
                    ask_master_password::Outcome::Ok(master_password) => master_password,
                    ask_master_password::Outcome::Cancelled => return Ok(None),
                    ask_master_password::Outcome::Custom(&()) => {
                        self.log_out()?;
                        break;
                    }
                };

                let result = Session::start(
                    self.http,
                    self.project_dirs.cache_dir(),
                    self.client_id,
                    auth::Device {
                        name: &*self.device_name,
                        identifier: self.data.device_id,
                        r#type: self.device_type,
                    },
                    &*email,
                    &**master_password,
                );

                match result {
                    Ok(session) => return Ok(Some(session)),
                    Err(session::StartError::Login(auth::login::Error {
                        kind: auth::login::ErrorKind::InvalidCredentials(_),
                        ..
                    })) => {}
                    Err(e) => return Err(e.into()),
                }

                again = true;
            }
        }
    }

    fn log_out(&mut self) -> anyhow::Result<()> {
        self.data.email = None;
        self.data.store().context("failed to log out")?;
        Ok(())
    }
}

enum AfterMenu {
    ShowMenuAgain {
        after_unlock: bool,
        menu_state: ipc::menu_request::MenuState,
    },
    ContinueServing,
    LogOut,
    StopServing,
}

struct MenuOpts {
    lib_dir: OsString,
    rofi_options: config::RofiOptions,
    copy_notification: bool,
    clipboard: Clipboard,
}

fn show_menu(
    session: &mut Session<'_, '_>,
    opts: &mut MenuOpts,
    display: &str,
    filter: &str,
) -> AfterMenu {
    try_show_menu(session, opts, display, filter).unwrap_or_else(|e| {
        report_error(e.context("failed to run menu").as_ref());
        AfterMenu::ContinueServing
    })
}

fn try_show_menu<'http, 'client_id>(
    session: &mut Session<'http, 'client_id>,
    opts: &mut MenuOpts,
    display: &str,
    filter: &str,
) -> anyhow::Result<AfterMenu> {
    let handshake = ipc::Handshake {
        master_key: session.master_key(),
        data: session.account_data().as_bytes(),
    };

    let res = menu::run(
        &*opts.lib_dir,
        &handshake,
        &opts.rofi_options,
        display,
        filter,
    )?;

    Ok(match res {
        ipc::MenuRequest::Copy {
            name,
            data,
            image_path,
            reprompt,
            menu_state,
        } => {
            if reprompt && !run_reprompt(session, &*name)? {
                return Ok(AfterMenu::ShowMenuAgain {
                    after_unlock: false,
                    menu_state,
                });
            }

            opts.clipboard
                .set_text(data)
                .context("failed to set clipboard content")?;

            if opts.copy_notification {
                show_notification(format!("copied {name} password"), image_path);
            }

            AfterMenu::ContinueServing
        }
        ipc::MenuRequest::Sync { menu_state } => {
            let after_unlock = match session.resync() {
                Ok(()) => false,
                Err(session::ResyncError::Refresh(auth::refresh::Error::SessionExpired(_))) => true,
                Err(e) => return Err(e.into()),
            };
            AfterMenu::ShowMenuAgain {
                after_unlock,
                menu_state,
            }
        }
        ipc::MenuRequest::Lock => AfterMenu::StopServing,
        ipc::MenuRequest::LogOut => AfterMenu::LogOut,
        ipc::MenuRequest::Exit => AfterMenu::ContinueServing,
    })
}

fn run_reprompt(session: &Session<'_, '_>, name: &str) -> anyhow::Result<bool> {
    let status =
        format!("The item \"{name}\" is protected and requires verifying your master password");

    let mut again = false;
    Ok(loop {
        let master_password = match ask_master_password::<Infallible>(again, &*status, &[])? {
            ask_master_password::Outcome::Ok(password) => password,
            ask_master_password::Outcome::Cancelled => break false,
            ask_master_password::Outcome::Custom(&unreachable) => match unreachable {},
        };
        if session.is_correct_master_password(&**master_password) {
            break true;
        }
        again = true;
    })
}

use ask_email::ask_email;
mod ask_email {
    pub(crate) fn ask_email() -> anyhow::Result<Option<String>> {
        let mut email = String::new();

        let mut dmenu = process::Command::new("rofi");
        dmenu.arg("-dmenu").stdin(process::Stdio::null());
        dmenu.arg("-p").arg("Email address");

        let outcome = run_dmenu(dmenu, &mut email).context("failed to prompt for email")?;

        if outcome == run_dmenu::Outcome::Cancelled || email.is_empty() {
            return Ok(None);
        }

        Ok(Some(email))
    }

    use crate::run_dmenu;
    use anyhow::Context as _;
    use std::process;
}

use ask_master_password::ask_master_password;
mod ask_master_password {
    pub(crate) fn ask_master_password<'keybinds, Action>(
        again: bool,
        status: &str,
        keybinds: &'keybinds [Keybind<Action>],
    ) -> anyhow::Result<Outcome<'keybinds, Action>> {
        // Try to prevent leaking of the master password into memory via a large buffer
        let mut master_password = Zeroizing::new(String::with_capacity(1024));

        let mut dmenu = process::Command::new("rofi");
        dmenu.arg("-dmenu").stdin(process::Stdio::null());
        let prompt = if again {
            "Master password incorrect, try again"
        } else {
            "Master password"
        };
        dmenu.arg("-p").arg(prompt);

        let mut message = String::new();
        if !keybinds.is_empty() {
            write!(message, "{}", keybind::HelpMarkup(keybinds)).unwrap();
        }
        if !status.is_empty() {
            if !message.is_empty() {
                message.push_str("\n\n");
            }
            message.push_str(status);
        }
        if !message.is_empty() {
            dmenu.arg("-mesg").arg(message);
        }

        keybind::apply_to_command(&mut dmenu, keybinds);

        dmenu.arg("-password");

        let outcome = run_dmenu(dmenu, &mut *master_password)
            .context("failed to prompt for master password")?;

        Ok(match outcome {
            run_dmenu::Outcome::Entered(_) if !master_password.is_empty() => {
                Outcome::Ok(master_password)
            }
            run_dmenu::Outcome::Custom(i) if usize::from(i) < keybinds.len() => {
                Outcome::Custom(&keybinds[usize::from(i)].action)
            }
            _ => Outcome::Cancelled,
        })
    }

    pub(crate) enum Outcome<'keybinds, Action> {
        Ok(Zeroizing<String>),
        Cancelled,
        Custom(&'keybinds Action),
    }

    use crate::run_dmenu;
    use anyhow::Context as _;
    use rofi_bw_common::keybind;
    use rofi_bw_common::Keybind;
    use std::fmt::Write as _;
    use std::process;
    use zeroize::Zeroizing;
}

use run_dmenu::run_dmenu;
mod run_dmenu {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Outcome {
        Entered(usize),
        Cancelled,
        Custom(u8),
    }

    pub(crate) fn run_dmenu(
        mut rofi: process::Command,
        buf: &mut String,
    ) -> anyhow::Result<Outcome> {
        rofi.stdout(process::Stdio::piped());

        let mut rofi = rofi.spawn().context("failed to spawn rofi")?;

        let mut stdout = rofi.stdout.take().unwrap();
        let mut bytes_read = stdout
            .read_to_string(buf)
            .context("failed to read Rofi's output")?;

        let status = rofi.wait().context("failed to wait on Rofi")?;

        match status.code() {
            Some(0) => {}
            Some(n @ 10..=28) => return Ok(Outcome::Custom(u8::try_from(n - 10).unwrap())),
            _ => return Ok(Outcome::Cancelled),
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
}

use show_notification::show_notification;
mod show_notification {
    pub(crate) fn show_notification(summary: String, image: Option<String>) {
        if let Err(e) = inner(summary, image) {
            eprintln!("Warning: {}", e.context("failed to show notification"));
        }
    }
    fn inner(summary: String, image: Option<String>) -> anyhow::Result<()> {
        let mut builder = notify_rust::Notification::new();
        builder.icon("bitwarden");
        builder.summary = summary;
        if let Some(image) = image {
            builder.hint(notify_rust::Hint::ImagePath(image));
        }
        builder.show().context("failed to show notification")?;
        Ok(())
    }

    use anyhow::Context as _;
}

mod daemon;

use session::Session;
mod session;

mod bitwarden_api;

mod cache;

mod config;

use data::Data;
mod data;

mod menu;

use error_reporting::report_error;
mod error_reporting;

mod auth;

use anyhow::Context as _;
use arboard::Clipboard;
use clap::Parser;
use config::Config;
use daemon::Daemon;
use directories::ProjectDirs;
use rofi_bw_common::fs;
use rofi_bw_common::ipc;
use rofi_bw_common::Keybind;
use std::convert::Infallible;
use std::env;
use std::ffi::OsString;
use std::process;
