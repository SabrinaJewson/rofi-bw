pub(crate) struct Initialized {
    key: SymmetricKey,
    view: View,
    ciphers: Vec<Cipher>,
    icons: BwIcons,
    error_message: String,
}

enum View {
    All,
    Cipher(usize),
}

struct Cipher {
    id: Uuid,
    name: String,
    icon: Option<Icon>,
    reprompt: bool,
    fields: Vec<Field>,
    default_copy: Option<usize>,
}

struct Field {
    name: Cow<'static, str>,
    display: Cow<'static, str>,
    copyable: Option<Copyable>,
}

enum Icon {
    Host(Arc<str>),
}

enum Copyable {
    Encrypted(CipherString<String>),
    Decrypted(String),
}

impl Initialized {
    pub(crate) fn new(master_key: &MasterKey, data: Data) -> anyhow::Result<Self> {
        let mut icons = BwIcons::new()?;

        let key = data.profile.key.decrypt(master_key)?;

        // TODO: Parallelize this
        let mut ciphers = Vec::new();
        for cipher in data.ciphers {
            if let Some(cipher) = process_cipher(cipher, &key, &mut icons)? {
                ciphers.push(cipher);
            }
        }

        ciphers.sort_unstable_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));

        Ok(Self {
            key,
            view: View::All,
            ciphers,
            icons,
            error_message: String::new(),
        })
    }

    pub(crate) const DISPLAY_NAME: &'static str = "bitwarden";

    pub(crate) fn status(&self, s: &mut rofi_mode::String) {
        match self.view {
            View::All => s.push_str("All ciphers"),
            View::Cipher(i) => s.push_str(&*self.ciphers[i].name),
        }

        s.push_str("\n");

        if !self.error_message.is_empty() {
            s.push_str(&*self.error_message);
        }
    }

    pub(crate) fn entries(&self) -> usize {
        match self.view {
            View::All => self.ciphers.len(),
            View::Cipher(i) => self.ciphers[i].fields.len(),
        }
    }

    pub(crate) fn entry_content(&self, line: usize) -> &str {
        match self.view {
            View::All => &*self.ciphers[line].name,
            View::Cipher(i) => &*self.ciphers[i].fields[line].display,
        }
    }

    pub(crate) fn entry_icon(&mut self, line: usize) -> Option<cairo::Surface> {
        match self.view {
            View::All => match self.ciphers[line].icon.as_ref()? {
                Icon::Host(host) => self.icons.surface(host),
            },
            View::Cipher(_) => None,
        }
    }

    pub(crate) fn ok_alt(&mut self, line: usize, input: &mut rofi_mode::String) {
        match self.view {
            View::All => {
                input.clear();
                self.view = View::Cipher(line);
            }
            View::Cipher(_) => {}
        }
    }

    pub(crate) fn ok(
        &mut self,
        line: usize,
        input: &mut rofi_mode::String,
    ) -> Option<ipc::MenuRequest> {
        let (cipher, field) = match self.view {
            View::All => {
                let cipher = &self.ciphers[line];
                match cipher.default_copy {
                    Some(default_copy) => (cipher, default_copy),
                    None => {
                        input.clear();
                        self.view = View::Cipher(line);
                        return None;
                    }
                }
            }
            View::Cipher(i) => {
                let cipher = &self.ciphers[i];
                (cipher, line)
            }
        };

        let field = &cipher.fields[field];

        let copyable = field.copyable.as_ref()?;

        let data = match copyable.decrypt(&self.key) {
            Ok(decrypted) => decrypted,
            Err(error) => {
                self.error_message = error_status(error.context(format!(
                    "failed to decrypt {} of {}",
                    field.name, cipher.name
                )));
                return None;
            }
        };

        let image_path = match &cipher.icon {
            Some(Icon::Host(host)) => self
                .icons
                .fs_path(host)
                .and_then(fs::Path::to_str)
                .map(str::to_owned),
            None => None,
        };

        Some(ipc::MenuRequest::Copy {
            cipher_name: cipher.name.clone(),
            field: field.name.clone().into_owned(),
            data,
            image_path,
            reprompt: match copyable {
                Copyable::Encrypted(_) => cipher.reprompt,
                Copyable::Decrypted(_) => false,
            },
            menu_state: ipc::menu_request::MenuState {
                filter: input.to_string(),
            },
        })
    }
}

fn process_cipher(
    cipher: data::Cipher,
    key: &SymmetricKey,
    icons: &mut BwIcons,
) -> anyhow::Result<Option<Cipher>> {
    if cipher.deleted_date.is_some() {
        return Ok(None);
    }

    let name = cipher.name.decrypt(key)?;

    let mut icon = None;
    let mut fields = Vec::new();
    let mut default_copy = None;

    match cipher.data {
        CipherData::Login(login) => {
            icon = extract_host(&login, key).map(Icon::Host);

            if let Some(username) = login.username {
                let username = username.decrypt(key)?;
                fields.push(Field {
                    name: Cow::Borrowed("username"),
                    display: Cow::Owned(format!("Username: {username}")),
                    copyable: Some(Copyable::Decrypted(username)),
                });
            }

            if let Some(password) = login.password {
                default_copy = Some(fields.len());
                fields.push(Field {
                    name: Cow::Borrowed("password"),
                    display: Cow::Borrowed("Password"),
                    copyable: Some(Copyable::Encrypted(password)),
                });
            }

            for uri in login.uris.into_iter().flatten() {
                let uri = uri.uri.decrypt(key)?;
                fields.push(Field {
                    name: Cow::Borrowed("uri"),
                    display: Cow::Owned(format!("Uri: {uri}")),
                    copyable: Some(Copyable::Decrypted(uri)),
                });
            }
        }
        CipherData::SecureNote => {}
        // TODO: Card and identity
        _ => return Ok(None),
    }

    if let Some(notes) = cipher.notes {
        let notes = notes.decrypt(key)?;
        fields.push(Field {
            name: Cow::Borrowed("note"),
            // TODO: Note preview
            display: Cow::Borrowed("Notes"),
            copyable: Some(Copyable::Decrypted(notes)),
        });
    }

    for custom_field in cipher.fields.into_iter().flatten() {
        let name = match custom_field.name {
            Some(name) => Some(Cow::Owned(name.decrypt(key)?)),
            None => None,
        };

        enum FieldValue {
            Text(Option<String>),
            Hidden(Option<CipherString<String>>),
            Boolean(bool),
            Linked(u32),
        }

        let value = match custom_field.value {
            data::FieldValue::Text(Some(v)) => FieldValue::Text(Some(v.decrypt(key)?)),
            data::FieldValue::Text(None) => FieldValue::Text(None),
            data::FieldValue::Hidden(v) => FieldValue::Hidden(v),
            data::FieldValue::Boolean(v) => FieldValue::Boolean(v.decrypt(key)?),
            data::FieldValue::Linked(v) => FieldValue::Linked(v),
        };

        let display_name = name.as_deref().unwrap_or(match value {
            FieldValue::Text(_) => "Text field",
            FieldValue::Hidden(_) => "Hidden field",
            FieldValue::Boolean(_) => "Boolean field",
            FieldValue::Linked(_) => "linked field",
        });

        let display = Cow::Owned(match &value {
            FieldValue::Text(Some(text)) => format!("{display_name}: {text}"),
            FieldValue::Text(None) => format!("{display_name} (empty)"),
            FieldValue::Hidden(Some(_)) => format!("{display_name} (hidden)"),
            FieldValue::Hidden(None) => format!("{display_name} (hidden, empty)"),
            FieldValue::Boolean(false) => format!("{display_name} ☐"),
            FieldValue::Boolean(true) => format!("{display_name} ☑"),
            FieldValue::Linked(v) => format!("{display_name} → {v}"),
        });

        let name = name.unwrap_or(Cow::Borrowed(match value {
            FieldValue::Text(_) => "text field",
            FieldValue::Hidden(_) => "hidden field",
            FieldValue::Boolean(_) => "boolean field",
            FieldValue::Linked(_) => "linked field",
        }));

        let copyable = match value {
            FieldValue::Text(Some(text)) => Some(Copyable::Decrypted(text)),
            FieldValue::Hidden(Some(hidden)) => Some(Copyable::Encrypted(hidden)),
            FieldValue::Hidden(None) | FieldValue::Text(None) => {
                Some(Copyable::Decrypted(String::new()))
            }
            FieldValue::Boolean(v) => Some(Copyable::Decrypted(v.to_string())),
            FieldValue::Linked(_) => None,
        };

        fields.push(Field {
            name,
            display,
            copyable,
        });
    }

    match &icon {
        Some(Icon::Host(host)) => icons.start_fetch(host.clone()),
        None => {}
    }

    Ok(Some(Cipher {
        id: cipher.id,
        name,
        icon,
        reprompt: cipher.reprompt,
        fields,
        default_copy,
    }))
}

impl Copyable {
    fn decrypt(&self, key: &SymmetricKey) -> anyhow::Result<String> {
        Ok(match self {
            Self::Encrypted(data) => data.decrypt(key)?,
            Self::Decrypted(data) => data.clone(),
        })
    }
}

fn extract_host(login: &data::Login, key: &SymmetricKey) -> Option<Arc<str>> {
    let url = login.uri.as_ref()?.decrypt(key).ok()?;
    let url = url.trim();
    // Algorithm taken from:
    // https://github.com/bitwarden/clients/blob/9eefb4ad169dc1ca08073922c78faafd12cb2752/libs/common/src/misc/utils.ts#L339
    let url = Url::parse(&*url).ok().or_else(|| {
        if url.starts_with("http://") || url.starts_with("https://") || !url.contains(".") {
            return None;
        }
        Url::parse(&*format!("http://{url}")).ok()
    })?;

    match url.host()? {
        url::Host::Domain(domain) => Some(<Arc<str>>::from(domain)),
        _ => None,
    }
}

use crate::cipher_string::CipherString;
use crate::data;
use crate::data::CipherData;
use crate::data::Data;
use crate::error_status::error_status;
use crate::symmetric_key::SymmetricKey;
use crate::BwIcons;
use rofi_bw_common::fs;
use rofi_bw_common::ipc;
use rofi_bw_common::MasterKey;
use rofi_mode::cairo;
use std::borrow::Cow;
use std::sync::Arc;
use url::Url;
use uuid::Uuid;
