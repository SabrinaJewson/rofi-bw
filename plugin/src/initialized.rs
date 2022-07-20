pub(crate) struct Initialized {
    entries: Vec<Entry>,
}

struct Entry {
    id: Uuid,
    name: String,
    password: Zeroizing<String>,
}

impl Initialized {
    pub(crate) fn new(master_key: &MasterKey, data: Data) -> anyhow::Result<Self> {
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

            entries.push(Entry {
                id: cipher.id,
                name,
                password,
            });
        }

        entries.sort_unstable_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));

        Ok(Self { entries })
    }

    pub(crate) const DISPLAY_NAME: &'static str = "bitwarden";

    pub(crate) fn entries(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn entry_content(&self, line: usize) -> &str {
        &*self.entries[line].name
    }

    // TODO: icons

    pub(crate) fn ok(&mut self, line: usize) -> ipc::MenuRequest<&str> {
        ipc::MenuRequest::Copy(&**self.entries[line].password)
    }
}

use crate::data::CipherData;
use crate::data::Data;
use anyhow::Context as _;
use rofi_bw_common::ipc;
use rofi_bw_common::MasterKey;
use uuid::Uuid;
use zeroize::Zeroizing;
