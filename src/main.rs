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
    clippy::match_bool
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
    config_file: Option<PathBuf>,
}

// TODO: This function is too big, I need to refactor it
#[allow(clippy::too_many_lines)]
fn try_main(
    Args {
        filter,
        config_file,
    }: Args,
) -> anyhow::Result<()> {
    let lib_dir = env::var_os("ROFI_BW_LIB_DIR")
        .unwrap_or_else(|| "/usr/lib/rofi-bw:/usr/local/lib/rofi-bw".into());

    let project_dirs = ProjectDirs::from("", "", "rofi-bw").context("no home directory")?;

    let runtime_dir = project_dirs
        .runtime_dir()
        .context("failed to locate runtime directory")?;

    let display = env::var("DISPLAY").context("failed to read `$DISPLAY` env var")?;

    let mut command = daemon::ShowMenu {
        display: &*display,
        filter: &*filter,
    };
    if daemon::invoke(runtime_dir, daemon::Command::ShowMenu(command))? {
        return Ok(());
    }

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

    let mut data = Data::load(project_dirs.data_dir())?;

    let http = ureq::agent();

    let mut clipboard = Clipboard::new().context("failed to open clipboard")?;

    loop {
        if data.email.is_none() {
            data.email = Some(match ask_email()? {
                Some(email) => email,
                None => return Ok(()),
            });
            data.store()?;
        }
        let email = data.email.as_ref().unwrap();

        let mut session = {
            let master_password = match ask_master_password()? {
                Some(master_password) => master_password,
                None => return Ok(()),
            };

            Session::new(
                &http,
                project_dirs.cache_dir(),
                &*client_id,
                auth::Device {
                    name: &*device_name,
                    identifier: data.device_id,
                    r#type: device_type,
                },
                &*email,
                &**master_password,
            )?
        };

        loop {
            enum AfterMenu {
                ShowMenuAgain,
                ContinueServing,
                UnlockAgain,
                StopServing,
            }

            let res: anyhow::Result<_> = (|| {
                let handshake = ipc::Handshake {
                    master_key: session.master_key(),
                    data: session.account_data().as_bytes(),
                    notify_copy: copy_notification,
                };

                let res = menu::run(
                    &*lib_dir,
                    &handshake,
                    &rofi_options,
                    command.display,
                    command.filter,
                )?;

                Ok(match res {
                    ipc::MenuRequest::Copy { data, notification } => {
                        clipboard
                            .set_text(data)
                            .context("failed to set clipboard content")?;

                        if let Some(notification) = notification {
                            show_notification(notification);
                        }

                        AfterMenu::ContinueServing
                    }
                    ipc::MenuRequest::Sync => match session.resync() {
                        Ok(()) => AfterMenu::ShowMenuAgain,
                        Err(session::ResyncError::RefreshToken(
                            auth::RefreshError::SessionExpired(_),
                        )) => AfterMenu::UnlockAgain,
                        Err(e) => return Err(e.into()),
                    },
                    ipc::MenuRequest::Lock => AfterMenu::StopServing,
                    ipc::MenuRequest::LogOut => {
                        data.email = None;
                        data.store()?;
                        AfterMenu::UnlockAgain
                    }
                    ipc::MenuRequest::Exit => AfterMenu::ContinueServing,
                })
            })();

            let after_menu = res.unwrap_or_else(|e| {
                report_error(e.context("failed to run menu").as_ref());
                AfterMenu::ContinueServing
            });

            match after_menu {
                AfterMenu::ShowMenuAgain => continue,
                AfterMenu::ContinueServing => {}
                AfterMenu::UnlockAgain => break,
                AfterMenu::StopServing => return Ok(()),
            }

            match daemon.wait() {
                daemon::Command::ShowMenu(new_command) => command = new_command,
                daemon::Command::Quit => return Ok(()),
            }
        }
    }
}

fn ask_email() -> anyhow::Result<Option<String>> {
    let mut email = String::new();
    if prompt("Email address", prompt::Visibility::Shown, &mut email)
        .context("failed to prompt for email")?
        == prompt::Outcome::Cancelled
        || email.is_empty()
    {
        return Ok(None);
    }
    Ok(Some(email))
}

fn ask_master_password() -> anyhow::Result<Option<Zeroizing<String>>> {
    // Try to prevent leaking of the master password into memory via a large buffer
    let mut master_password = Zeroizing::new(String::with_capacity(1024));
    if prompt(
        "Master password",
        prompt::Visibility::Hidden,
        &mut *master_password,
    )
    .context("failed to prompt for master password")?
        == prompt::Outcome::Cancelled
        || master_password.is_empty()
    {
        return Ok(None);
    }
    Ok(Some(master_password))
}

use prompt::prompt;
mod prompt {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Visibility {
        Shown,
        Hidden,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Outcome {
        Entered(usize),
        Cancelled,
    }

    pub(crate) fn prompt(
        msg: &str,
        visibility: Visibility,
        buf: &mut String,
    ) -> anyhow::Result<Outcome> {
        let mut rofi = process::Command::new("rofi");
        rofi.arg("-dmenu");
        rofi.stdin(process::Stdio::null())
            .stdout(process::Stdio::piped());
        rofi.arg("-p").arg(msg);
        if visibility == Visibility::Hidden {
            rofi.arg("-password");
        }
        let mut rofi = rofi.spawn().context("failed to spawn rofi")?;

        let mut stdout = rofi.stdout.take().unwrap();
        let mut bytes_read = stdout
            .read_to_string(buf)
            .context("failed to read Rofi's output")?;

        let status = rofi.wait().context("failed to wait on Rofi")?;
        if !status.success() {
            return Ok(Outcome::Cancelled);
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
    pub(crate) fn show_notification(notification: ipc::menu_request::Notification) {
        if let Err(e) = inner(notification) {
            eprintln!("Warning: {}", e.context("failed to show notification"));
        }
    }
    fn inner(notification: ipc::menu_request::Notification) -> anyhow::Result<()> {
        let mut builder = notify_rust::Notification::new();
        builder.icon("bitwarden");
        builder.summary = notification.title;
        if let Some(image) = notification.image {
            builder.hint(notify_rust::Hint::ImagePath(image));
        }
        builder.show().context("failed to show notification")?;
        Ok(())
    }

    use anyhow::Context as _;
    use rofi_bw_common::ipc;
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
use rofi_bw_common::ipc;
use std::env;
use std::path::PathBuf;
use std::process;
use zeroize::Zeroizing;
