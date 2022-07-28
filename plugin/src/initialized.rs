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
    display: Cow<'static, str>,
    action: Option<Action>,
}

enum Action {
    Copy {
        name: Cow<'static, str>,
        data: Copyable,
    },
    Link {
        to: &'static str,
    },
}

enum Copyable {
    Encrypted(CipherString<String>),
    Decrypted(String),
}

enum Icon {
    Host(Arc<str>),
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

        match field.action.as_ref()? {
            Action::Copy { name, data } => {
                let reprompt = match data {
                    Copyable::Encrypted(_) => cipher.reprompt,
                    Copyable::Decrypted(_) => false,
                };

                let data = match data.decrypt(&self.key) {
                    Ok(decrypted) => decrypted,
                    Err(error) => {
                        self.error_message = error_status(
                            error.context(format!("failed to decrypt {name} of {}", cipher.name)),
                        );
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
                    field: name.clone().into_owned(),
                    data,
                    image_path,
                    reprompt,
                    menu_state: ipc::menu_request::MenuState {
                        filter: input.to_string(),
                    },
                })
            }
            Action::Link { to } => {
                input.clear();
                input.push_str(to);
                None
            }
        }
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

    let mut fields = Vec::new();
    let mut default_copy = None;

    let icon = match cipher.data {
        CipherData::Login(login) => process_login(login, key, &mut fields, &mut default_copy)?,
        CipherData::SecureNote => None,
        CipherData::Card(card) => process_card(card, key, &mut fields)?,
        CipherData::Identity(identity) => process_identity(identity, key, &mut fields)?,
    };

    match &icon {
        Some(Icon::Host(host)) => icons.start_fetch(host.clone()),
        None => {}
    }

    if let Some(notes) = cipher.notes {
        fields.push(Field::notes(notes.decrypt(key)?));
    }

    for custom_field in cipher.fields.into_iter().flatten() {
        let name = match custom_field.name {
            Some(name) => Some(name.decrypt(key)?),
            None => None,
        };

        let value = match custom_field.value {
            data::FieldValue::Text(Some(v)) => FieldValue::Text(Some(v.decrypt(key)?)),
            data::FieldValue::Text(None) => FieldValue::Text(None),
            data::FieldValue::Hidden(v) => FieldValue::Hidden(v),
            data::FieldValue::Boolean(v) => FieldValue::Boolean(v.decrypt(key)?),
            data::FieldValue::Linked(v) => FieldValue::Linked(v),
        };

        fields.push(Field::custom(name, value));
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

fn process_login(
    login: data::Login,
    key: &SymmetricKey,
    fields: &mut Vec<Field>,
    default_copy: &mut Option<usize>,
) -> anyhow::Result<Option<Icon>> {
    let icon = extract_host(&login, key).map(Icon::Host);

    if let Some(username) = login.username {
        fields.push(Field::username(username.decrypt(key)?));
    }

    if let Some(password) = login.password {
        *default_copy = Some(fields.len());
        fields.push(Field::password(password));
    }

    for uri in login.uris.into_iter().flatten() {
        fields.push(Field::uri(uri.uri.decrypt(key)?));
    }

    Ok(icon)
}

fn process_card(
    card: data::Card,
    key: &SymmetricKey,
    fields: &mut Vec<Field>,
) -> anyhow::Result<Option<Icon>> {
    // TODO: Card icons

    if let Some(cardholder_name) = card.cardholder_name {
        fields.push(Field::cardholder_name(cardholder_name.decrypt(key)?));
    }

    if let Some(brand) = card.brand {
        fields.push(Field::card_brand(brand.decrypt(key)?));
    }

    if let Some(number) = card.number {
        fields.push(Field::card_number(number));
    }

    if card.exp_month.is_some() || card.exp_year.is_some() {
        fields.push(Field::card_expiration(
            card.exp_month.map(|exp| exp.decrypt(key)).transpose()?,
            card.exp_year.map(|exp| exp.decrypt(key)).transpose()?,
        ));
    }

    if let Some(code) = card.code {
        fields.push(Field::card_code(code));
    }

    Ok(None)
}

fn process_identity(
    identity: data::Identity,
    key: &SymmetricKey,
    fields: &mut Vec<Field>,
) -> anyhow::Result<Option<Icon>> {
    if identity.title.is_some()
        || identity.first_name.is_some()
        || identity.middle_name.is_some()
        || identity.last_name.is_some()
    {
        fields.push(Field::identity_name(
            identity.title.map(|v| v.decrypt(key)).transpose()?,
            identity.first_name.map(|v| v.decrypt(key)).transpose()?,
            identity.middle_name.map(|v| v.decrypt(key)).transpose()?,
            identity.last_name.map(|v| v.decrypt(key)).transpose()?,
        ));
    }

    if let Some(username) = identity.username {
        fields.push(Field::identity_username(username.decrypt(key)?));
    }

    if let Some(company) = identity.company {
        fields.push(Field::identity_company(company.decrypt(key)?));
    }

    if let Some(ssn) = identity.ssn {
        fields.push(Field::identity_ssn(ssn.decrypt(key)?));
    }

    if let Some(number) = identity.passport_number {
        fields.push(Field::identity_passport_number(number.decrypt(key)?));
    }

    if let Some(licence_number) = identity.license_number {
        fields.push(Field::identity_licence_number(licence_number.decrypt(key)?));
    }

    if let Some(email) = identity.email {
        fields.push(Field::identity_email(email.decrypt(key)?));
    }

    if let Some(phone) = identity.phone {
        fields.push(Field::identity_phone(phone.decrypt(key)?));
    }

    if identity.address1.is_some()
        || identity.address2.is_some()
        || identity.address3.is_some()
        || identity.city.is_some()
        || identity.state.is_some()
        || identity.postal_code.is_some()
        || identity.country.is_some()
    {
        fields.push(Field::identity_address(
            identity.address1.map(|v| v.decrypt(key)).transpose()?,
            identity.address2.map(|v| v.decrypt(key)).transpose()?,
            identity.address3.map(|v| v.decrypt(key)).transpose()?,
            identity.city.map(|v| v.decrypt(key)).transpose()?,
            identity.state.map(|v| v.decrypt(key)).transpose()?,
            identity.postal_code.map(|v| v.decrypt(key)).transpose()?,
            identity.country.map(|v| v.decrypt(key)).transpose()?,
        ));
    }

    Ok(None)
}

impl Field {
    fn username(username: String) -> Self {
        Self::shown("Username", "username", username)
    }
    fn password(password: CipherString<String>) -> Self {
        Self::hidden("Password", "password", password)
    }
    fn uri(uri: String) -> Self {
        Self::shown("Uri", "URI", uri)
    }
    fn cardholder_name(name: String) -> Self {
        Self::shown("Cardholder name", "cardholder name", name)
    }
    fn card_brand(brand: String) -> Self {
        Self::shown("Brand", "brand", brand)
    }
    fn card_number(number: CipherString<String>) -> Self {
        Self::hidden("Number", "number", number)
    }
    fn card_expiration(month: Option<String>, year: Option<String>) -> Self {
        // TODO: Internationalize this?
        let expiration = format!(
            "{} / {}",
            month.as_deref().unwrap_or("__"),
            year.as_deref().unwrap_or("____"),
        );
        Self::shown("Expiration", "expiration", expiration)
    }
    fn card_code(code: CipherString<String>) -> Self {
        Self::hidden("Security code", "security code", code)
    }
    fn identity_name(
        title: Option<String>,
        first_name: Option<String>,
        middle_name: Option<String>,
        last_name: Option<String>,
    ) -> Self {
        let mut name = String::new();
        for part in [title, first_name, middle_name, last_name]
            .into_iter()
            .flatten()
        {
            if name.is_empty() {
                name = part;
            } else {
                name.push(' ');
                name.push_str(&*part);
            }
        }
        Self::shown("Identity name", "name", name)
    }
    fn identity_username(username: String) -> Self {
        Self::shown("Username", "username", username)
    }
    fn identity_company(company: String) -> Self {
        Self::shown("Company", "company", company)
    }
    fn identity_ssn(ssn: String) -> Self {
        // TODO: Internationalize
        Self::shown(
            "National Insurance number",
            "national insurance number",
            ssn,
        )
    }
    fn identity_passport_number(number: String) -> Self {
        Self::shown("Passport number", "passport number", number)
    }
    fn identity_licence_number(licence_number: String) -> Self {
        Self::shown("Licence number", "licence number", licence_number)
    }
    fn identity_email(email: String) -> Self {
        Self::shown("Email", "email", email)
    }
    fn identity_phone(phone: String) -> Self {
        Self::shown("Phone", "phone", phone)
    }
    fn identity_address(
        address1: Option<String>,
        address2: Option<String>,
        address3: Option<String>,
        city: Option<String>,
        state: Option<String>,
        postal_code: Option<String>,
        country: Option<String>,
    ) -> Self {
        // TODO: Internationalize
        let mut data = String::new();

        for row in [
            [address1].iter().flatten(),
            [address2].iter().flatten(),
            [address3].iter().flatten(),
            [city, state, postal_code].iter().flatten(),
            [country].iter().flatten(),
        ] {
            for (i, item) in row.enumerate() {
                if i > 0 {
                    data.push_str(", ");
                }
                data.push_str(item);
            }
            data.push_str("\n");
        }

        Self {
            display: if let Some(first_line) = data.lines().next() {
                Cow::Owned(format!("Address: {first_line}…"))
            } else {
                Cow::Borrowed("Address")
            },
            action: Some(Action::Copy {
                name: Cow::Borrowed("address"),
                data: Copyable::Decrypted(data),
            }),
        }
    }
    fn notes(notes: String) -> Self {
        Self {
            // TODO: Note preview
            display: Cow::Borrowed("Notes"),
            action: Some(Action::Copy {
                name: Cow::Borrowed("note"),
                data: Copyable::Decrypted(notes),
            }),
        }
    }

    fn custom(name: Option<String>, value: FieldValue) -> Self {
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

        let name = name.map(Cow::Owned);

        let action = match value {
            FieldValue::Text(text) => Some(Action::Copy {
                name: name.unwrap_or(Cow::Borrowed("text field")),
                data: Copyable::Decrypted(text.unwrap_or_default()),
            }),
            FieldValue::Hidden(hidden) => Some(Action::Copy {
                name: name.unwrap_or(Cow::Borrowed("hidden field")),
                data: match hidden {
                    Some(hidden) => Copyable::Encrypted(hidden),
                    None => Copyable::Decrypted(String::new()),
                },
            }),
            FieldValue::Boolean(v) => Some(Action::Copy {
                name: name.unwrap_or(Cow::Borrowed("boolean field")),
                data: Copyable::Decrypted(v.to_string()),
            }),
            FieldValue::Linked(to) => Some(Action::Link { to: to.as_str() }),
        };

        Self { display, action }
    }

    fn shown(title: &'static str, name: &'static str, value: String) -> Self {
        Self {
            display: Cow::Owned(format!("{title}: {value}")),
            action: Some(Action::Copy {
                name: Cow::Borrowed(name),
                data: Copyable::Decrypted(value),
            }),
        }
    }

    fn hidden(title: &'static str, name: &'static str, value: CipherString<String>) -> Self {
        Self {
            display: Cow::Borrowed(title),
            action: Some(Action::Copy {
                name: Cow::Borrowed(name),
                data: Copyable::Encrypted(value),
            }),
        }
    }
}

enum FieldValue {
    Text(Option<String>),
    Hidden(Option<CipherString<String>>),
    Boolean(bool),
    Linked(data::Linked),
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
