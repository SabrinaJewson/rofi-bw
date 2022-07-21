pub(crate) struct Initialized {
    entries: Vec<Entry>,
    icons: BwIcons,
}

struct Entry {
    id: Uuid,
    name: String,
    password: Zeroizing<String>,
    host: Option<Arc<str>>,
}

impl Initialized {
    pub(crate) fn new(master_key: &MasterKey, data: Data) -> anyhow::Result<Self> {
        let mut icons = BwIcons::new()?;

        let key = data.profile.key.decrypt(master_key)?;

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

            let password = password
                .decrypt(&key)
                .with_context(|| format!("failed to decryt password of `{name}`"))?;
            let password = Zeroizing::new(password);

            let host = (|| {
                let url = login.uri.as_ref()?.decrypt(&key).ok()?;
                let url = Url::parse(&*url).ok()?;
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

        Ok(Self { entries, icons })
    }

    pub(crate) const DISPLAY_NAME: &'static str = "bitwarden";

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

    pub(crate) fn ok(&mut self, line: usize) -> ipc::MenuRequest<&str> {
        ipc::MenuRequest::Copy(&**self.entries[line].password)
    }
}

use crate::data::CipherData;
use crate::data::Data;
use crate::BwIcons;
use anyhow::Context as _;
use rofi_bw_common::ipc;
use rofi_bw_common::MasterKey;
use rofi_mode::cairo;
use std::sync::Arc;
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;
