pub(crate) struct Initialized {
    key: SymmetricKey,
    entries: Vec<Entry>,
    icons: BwIcons,
    notify_copy: bool,
    error_message: String,
}

struct Entry {
    id: Uuid,
    name: String,
    password: CipherString<String>,
    host: Option<Arc<str>>,
}

impl Initialized {
    pub(crate) fn new(
        master_key: &MasterKey,
        data: Data,
        notify_copy: bool,
    ) -> anyhow::Result<Self> {
        let mut icons = BwIcons::new()?;

        let key = data.profile.key.decrypt(master_key)?;

        // TODO: Parallelize this
        let mut entries = Vec::new();
        for cipher in data.ciphers {
            if cipher.deleted_date.is_some() {
                continue;
            }
            let login = match cipher.data {
                CipherData::Login(login) => login,
                _ => continue,
            };
            let password = match login.password {
                Some(password) => password,
                None => continue,
            };

            let id = &cipher.id;

            let name = cipher
                .name
                .decrypt(&key)
                .with_context(|| format!("failed to decrypt name of cipher {id}"))?;

            let host = (|| {
                let url = login.uri.as_ref()?.decrypt(&key).ok()?;
                let url = url.trim();
                // Algorithm taken from:
                // https://github.com/bitwarden/clients/blob/9eefb4ad169dc1ca08073922c78faafd12cb2752/libs/common/src/misc/utils.ts#L339
                let url = Url::parse(&*url).ok().or_else(|| {
                    if url.starts_with("http://")
                        || url.starts_with("https://")
                        || !url.contains(".")
                    {
                        return None;
                    }
                    Url::parse(&*format!("http://{url}")).ok()
                })?;

                match url.host()? {
                    url::Host::Domain(domain) => Some(<Arc<str>>::from(domain)),
                    _ => None,
                }
            })();

            if let Some(host) = host.clone() {
                icons.start_fetch(host);
            }

            entries.push(Entry {
                id: cipher.id,
                name,
                password,
                host,
            });
        }

        entries.sort_unstable_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));

        Ok(Self {
            key,
            entries,
            icons,
            notify_copy,
            error_message: String::new(),
        })
    }

    pub(crate) const DISPLAY_NAME: &'static str = "bitwarden";

    pub(crate) fn error(&self) -> Option<&str> {
        if self.error_message.is_empty() {
            return None;
        }
        Some(&*self.error_message)
    }

    pub(crate) fn entries(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn entry_content(&self, line: usize) -> &str {
        &*self.entries[line].name
    }

    pub(crate) fn entry_icon(&mut self, line: usize) -> Option<cairo::Surface> {
        let host = self.entries[line].host.as_deref()?;
        self.icons.get(host)
    }

    pub(crate) fn ok(&mut self, line: usize) -> Option<ipc::MenuRequest<String>> {
        let entry = &self.entries[line];

        let password = match entry.password.decrypt(&self.key) {
            Ok(password) => password,
            Err(error) => {
                self.error_message = error_status(
                    anyhow!(error).context(format!("failed to decrypt password of {}", entry.name)),
                );
                return None;
            }
        };

        Some(ipc::MenuRequest::Copy {
            data: password,
            notification: self.notify_copy.then(|| ipc::menu_request::Notification {
                title: format!("copied {} password", entry.name),
                image: entry
                    .host
                    .as_deref()
                    .and_then(|host| self.icons.fs_path(host))
                    .and_then(|path| path.into_os_string().into_string().ok()),
            }),
        })
    }
}

use crate::cipher_string::CipherString;
use crate::data::CipherData;
use crate::data::Data;
use crate::error_status::error_status;
use crate::symmetric_key::SymmetricKey;
use crate::BwIcons;
use anyhow::anyhow;
use anyhow::Context as _;
use rofi_bw_common::ipc;
use rofi_bw_common::MasterKey;
use rofi_mode::cairo;
use std::sync::Arc;
use url::Url;
use uuid::Uuid;
