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
    clippy::items_after_statements
)]

fn main() -> process::ExitCode {
    if let Err(e) = try_main() {
        report_error(e.as_ref());
        return process::ExitCode::FAILURE;
    }
    process::ExitCode::SUCCESS
}

fn try_main() -> anyhow::Result<()> {
    let lib_dir = env::var_os("ROFI_BW_LIB_DIR")
        .unwrap_or_else(|| "/usr/lib/rofi-bw:/usr/local/lib/rofi-bw".into());

    let project_dirs = ProjectDirs::from("", "", "rofi-bw").context("no home directory")?;

    let runtime_dir = project_dirs
        .runtime_dir()
        .context("failed to locate runtime directory")?;

    if daemon::invoke(runtime_dir)? {
        return Ok(());
    }

    // Having failed to invoke an existing daemon, we must now become the daemon.

    let mut daemon = Daemon::bind(runtime_dir)?;

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

        let (master_key, token) = {
            let master_password = match ask_master_password()? {
                Some(master_password) => master_password,
                None => return Ok(()),
            };

            unlock_or_log_in(
                &http,
                project_dirs.cache_dir(),
                &data.device_id,
                &*email,
                &**master_password,
            )?
        };

        let token_source = TokenSource {
            http: http.clone(),
            token,
        };
        let mut client = Client {
            http: http.clone(),
            token_source,
            base_url: "https://vault.bitwarden.com/api",
        };
        let mut account_data = client.sync()?;

        loop {
            enum AfterMenu {
                ShowMenuAgain,
                ContinueServing,
                UnlockAgain,
                StopServing,
            }

            let res: anyhow::Result<_> = (|| {
                Ok(match menu::run(&*lib_dir, &master_key, &*account_data)? {
                    ipc::MenuRequest::Copy(data) => {
                        clipboard
                            .set_text(String::from(data))
                            .context("failed to set clipboard content")?;
                        AfterMenu::ContinueServing
                    }
                    ipc::MenuRequest::Sync => {
                        // Force a token refresh. This is needed to make sure that our session
                        // hasn't expired; if it has, it’s likely the master password or KDF
                        // iterations have changed, and so we need to enter the master password
                        // again.
                        client.token_source.token.access_token.clear();

                        match client.sync() {
                            Ok(new_account_data) => {
                                account_data = new_account_data;
                                AfterMenu::ShowMenuAgain
                            }
                            Err(client::SyncError::Token(auth::RefreshError::SessionExpired(
                                _,
                            ))) => AfterMenu::UnlockAgain,
                            Err(e) => return Err(e.into()),
                        }
                    }
                    ipc::MenuRequest::Lock => AfterMenu::StopServing,
                    ipc::MenuRequest::LogOut => {
                        data.email = None;
                        data.store()?;
                        AfterMenu::UnlockAgain
                    }
                    ipc::MenuRequest::Exit => AfterMenu::ContinueServing,
                })
            })();

            let after_menu = match res {
                Ok(after_menu) => after_menu,
                Err(e) => {
                    report_error(e.context("failed to run menu").as_ref());
                    AfterMenu::ContinueServing
                }
            };

            match after_menu {
                AfterMenu::ShowMenuAgain => continue,
                AfterMenu::ContinueServing => {}
                AfterMenu::UnlockAgain => break,
                AfterMenu::StopServing => return Ok(()),
            }

            daemon.wait();
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

/// If we have already started a session on this device, unlock that session; otherwise, log in.
fn unlock_or_log_in(
    http: &ureq::Agent,
    cache_dir: &Path,
    device_id: &Uuid,
    email: &str,
    master_password: &str,
) -> anyhow::Result<(MasterKey, AccessToken)> {
    let cache_key = cache::Key::new(email, master_password)?;
    let cache = cache::load(cache_dir, &cache_key);

    let validated_cache = match cache {
        Some(cache) => match auth::refresh_token(http, CLIENT_ID, &*cache.refresh_token) {
            Ok(token) => Some((cache.prelogin, token)),
            Err(auth::RefreshError::SessionExpired(_)) => None,
            Err(e) => return Err(e.into()),
        },
        None => None,
    };

    Ok(match validated_cache {
        Some((prelogin, token)) => {
            let master_key = auth::master_key(&prelogin, email, master_password);
            (master_key, token)
        }
        None => {
            let (prelogin, master_key, token) = auth::login(
                http,
                CLIENT_ID,
                auth::Device {
                    name: "linux",
                    identifier: &*format!("{:x}", device_id.as_hyphenated()),
                    r#type: auth::DeviceType::LinuxDesktop,
                },
                auth::Scopes::all(),
                email,
                master_password,
            )?;
            cache::store(
                cache_dir,
                &cache_key,
                CacheRef {
                    refresh_token: &*token.refresh_token,
                    prelogin: &prelogin,
                },
            );
            (master_key, token)
        }
    })
}

const CLIENT_ID: &str = "desktop";

struct TokenSource {
    http: ureq::Agent,
    token: AccessToken,
}

impl client::TokenSource for TokenSource {
    type Error = auth::RefreshError;
    fn access_token(&mut self) -> Result<&str, Self::Error> {
        if self.token.is_expired() {
            self.token = auth::refresh_token(&self.http, CLIENT_ID, &*self.token.refresh_token)?;
        }
        Ok(&*self.token.access_token)
    }
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

mod daemon;

use auth::AccessToken;
mod auth;

mod client;

mod cache;

use data::Data;
mod data;

mod fs_overwrite;

mod menu;

use error_reporting::report_error;
mod error_reporting;

use crate::cache::CacheRef;
use anyhow::Context as _;
use arboard::Clipboard;
use client::Client;
use daemon::Daemon;
use directories::ProjectDirs;
use rofi_bw_common::ipc;
use rofi_bw_common::MasterKey;
use std::env;
use std::path::Path;
use std::process;
use uuid::Uuid;
use zeroize::Zeroizing;
