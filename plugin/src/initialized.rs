pub(crate) struct Initialized {
    state: State,
    icons: Icons,
    // Currently unused, but may be useful in future
    error_message: String,
}

impl Initialized {
    pub(crate) fn new(master_key: &MasterKey, data: Data, view: ipc::View) -> anyhow::Result<Self> {
        let mut icons = Icons::new()?;

        let state = State::new(master_key, data, view)?;

        for cipher in &state.ciphers {
            icons.start_fetch(&cipher.icon);
        }

        Ok(Self {
            state,
            icons,
            error_message: String::new(),
        })
    }
}

impl Initialized {
    pub(crate) const DISPLAY_NAME: &'static str = "bitwarden";

    pub(crate) fn status(&self, s: &mut rofi_mode::String) {
        s.push_str(self.state.viewing().description());
        s.push_str("\n");

        if !self.error_message.is_empty() {
            s.push_str(&*self.error_message);
        }
    }

    pub(crate) fn entries(&self) -> usize {
        match self.state.viewing() {
            Viewing::CipherList(list) => list.contents.len(),
            Viewing::Cipher(cipher) => cipher.fields.len(),
        }
    }

    pub(crate) fn entry_content(&self, line: usize) -> &str {
        match self.state.viewing() {
            Viewing::CipherList(list) => &*self.state.ciphers[list.contents[line]].name,
            Viewing::Cipher(cipher) => &*cipher.fields[line].display,
        }
    }

    pub(crate) fn entry_icon(&mut self, line: usize, height: u32) -> Option<cairo::Surface> {
        let icon = match self.state.viewing() {
            Viewing::CipherList(list) => &self.state.ciphers[list.contents[line]].icon,
            Viewing::Cipher(cipher) => &cipher.fields[line].icon,
        };
        self.icons.surface(icon, height)
    }

    pub(crate) fn show(&mut self, list: CipherList) {
        self.state.view = View::CipherList(list);
    }

    pub(crate) fn ok_alt(&mut self, line: usize, input: &mut rofi_mode::String) {
        match self.state.viewing() {
            Viewing::CipherList(list) => {
                input.clear();
                self.state.view = View::Cipher(list.contents[line]);
            }
            Viewing::Cipher(_) => {}
        }
    }

    pub(crate) fn ok(
        &mut self,
        line: usize,
        input: &mut rofi_mode::String,
    ) -> Option<ipc::MenuRequest> {
        let (cipher, field) = match self.state.viewing() {
            Viewing::CipherList(list) => {
                let cipher = &self.state.ciphers[list.contents[line]];
                match cipher.default_copy {
                    Some(default_copy) => (cipher, default_copy),
                    None => {
                        input.clear();
                        self.state.view = View::Cipher(list.contents[line]);
                        return None;
                    }
                }
            }
            Viewing::Cipher(cipher) => (cipher, line),
        };

        let field = &cipher.fields[field];

        match field.action.as_ref()? {
            Action::Copy { name, data, hidden } => {
                let cipher_name = cipher.name.clone();
                let reprompt = *hidden && cipher.reprompt;

                let image_path = self
                    .icons
                    .fs_path(&cipher.icon)
                    .and_then(|path| std::fs::canonicalize(path).ok())
                    .and_then(|path| path.into_os_string().into_string().ok());

                Some(ipc::MenuRequest::Copy {
                    cipher_name,
                    field: name.clone().into_owned(),
                    data: data.to_string(),
                    image_path,
                    reprompt,
                    menu_state: ipc::menu_request::MenuState {
                        filter: input.to_string(),
                        view: self.ipc_view(),
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

    pub(crate) fn ipc_view(&self) -> ipc::View {
        match self.state.viewing() {
            Viewing::CipherList(list) => ipc::View::CipherList(list.list),
            Viewing::Cipher(cipher) => {
                ipc::View::Cipher(ipc::CipherFilter::Uuid(cipher.id.into_bytes()))
            }
        }
    }
}

struct State {
    view: View,
    ciphers: CipherSet,
    all: Vec<cipher_set::Index>,
    trash: Vec<cipher_set::Index>,
    favourites: Vec<cipher_set::Index>,
    type_buckets: CipherTypeList<Vec<cipher_set::Index>>,
}

#[derive(Clone, Copy)]
enum View {
    CipherList(CipherList),
    Cipher(cipher_set::Index),
    // TODO: folder view
}

impl State {
    pub(crate) fn new(master_key: &MasterKey, data: Data, view: ipc::View) -> anyhow::Result<Self> {
        let key = data.profile.key.decrypt(master_key)?;

        let mut ciphers = (0..data.ciphers.len())
            .map(|_| Cipher::safe_uninit())
            .collect::<Box<[_]>>();

        parallel_try_fill(
            data.ciphers
                .into_par_iter()
                .map(|cipher| process_cipher(cipher, &key)),
            &mut *ciphers,
        )?;

        // TODO: Use a proper Unicode sort
        ciphers.sort_unstable_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));

        let ciphers = CipherSet::from_boxed_slice(ciphers);

        let mut all = Vec::new();
        let mut trash = Vec::new();
        let mut favourites = Vec::new();
        let mut type_buckets = <CipherTypeList<Vec<_>>>::default();
        for (i, cipher) in ciphers.enumerated() {
            if cipher.deleted {
                trash.push(i);
            } else {
                if cipher.favourite {
                    favourites.push(i);
                }
                all.push(i);
            }

            type_buckets[cipher.r#type].push(i);
        }

        Ok(Self {
            view: match view {
                ipc::View::CipherList(list) => View::CipherList(list),
                ipc::View::Cipher(filter) => {
                    if let Some(index) = ciphers.filter(&filter) {
                        View::Cipher(index)
                    } else {
                        View::CipherList(CipherList::All)
                    }
                }
            },
            ciphers,
            all,
            trash,
            favourites,
            type_buckets,
        })
    }

    pub(crate) fn viewing(&self) -> Viewing<'_> {
        match self.view {
            View::CipherList(list) => Viewing::CipherList(CipherListRef {
                list,
                contents: match list {
                    CipherList::All => &*self.all,
                    CipherList::Trash => &*self.trash,
                    CipherList::Favourites => &*self.favourites,
                    CipherList::TypeBucket(cipher_type) => &*self.type_buckets[cipher_type],
                },
            }),
            View::Cipher(i) => Viewing::Cipher(&self.ciphers[i]),
        }
    }
}

enum Viewing<'a> {
    CipherList(CipherListRef<'a>),
    Cipher(&'a Cipher),
}

impl Viewing<'_> {
    fn description(&self) -> &str {
        match self {
            Self::CipherList(list) => list.list.description(),
            Self::Cipher(cipher) => &*cipher.name,
        }
    }
}

struct CipherListRef<'a> {
    list: CipherList,
    contents: &'a [cipher_set::Index],
}

fn process_cipher(cipher: data::Cipher, key: &SymmetricKey) -> anyhow::Result<Cipher> {
    let name = cipher.name.decrypt(key)?;

    let mut fields = Vec::new();
    let mut default_copy = None;
    let icon;

    let r#type = match cipher.data {
        CipherData::Login(login) => {
            icon = process_login(login, key, &mut fields, &mut default_copy)?;
            CipherType::Login
        }
        CipherData::SecureNote => {
            // The default copy of a secure note should be its note, unlike any other cipher data
            // type. This works because the next field we add is always the note field.
            if cipher.notes.is_some() {
                default_copy = Some(fields.len());
            }
            icon = Icon::Glyph(icons::Glyph::SecureNote);
            CipherType::SecureNote
        }
        CipherData::Card(card) => {
            icon = process_card(card, key, &mut fields)?;
            CipherType::Card
        }
        CipherData::Identity(identity) => {
            icon = process_identity(identity, key, &mut fields)?;
            CipherType::Identity
        }
    };

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
            data::FieldValue::Hidden(Some(v)) => FieldValue::Hidden(Some(v.decrypt(key)?)),
            data::FieldValue::Hidden(None) => FieldValue::Hidden(None),
            data::FieldValue::Boolean(v) => FieldValue::Boolean(v.decrypt(key)?),
            data::FieldValue::Linked(v) => FieldValue::Linked(v),
        };

        fields.push(Field::custom(name, value));
    }

    Ok(Cipher {
        id: cipher.id,
        r#type,
        deleted: cipher.deleted_date.is_some(),
        favourite: cipher.favourite,
        name,
        icon,
        reprompt: cipher.reprompt,
        fields,
        default_copy,
    })
}

fn process_login(
    login: data::Login,
    key: &SymmetricKey,
    fields: &mut Vec<Field>,
    default_copy: &mut Option<usize>,
) -> anyhow::Result<Icon> {
    let icon = extract_host(&login, key).map_or(Icon::Glyph(icons::Glyph::Login), Icon::Host);

    if let Some(username) = login.username {
        fields.push(Field::username(username.decrypt(key)?));
    }

    if let Some(password) = login.password {
        *default_copy = Some(fields.len());
        fields.push(Field::password(password.decrypt(key)?));
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
) -> anyhow::Result<Icon> {
    let mut icon = None;

    if let Some(cardholder_name) = card.cardholder_name {
        fields.push(Field::cardholder_name(cardholder_name.decrypt(key)?));
    }

    if let Some(brand) = card.brand {
        let brand = brand.decrypt(key)?;
        icon = Icon::card(&*brand);
        fields.push(Field::card_brand(brand));
    }

    if let Some(number) = card.number {
        fields.push(Field::card_number(number.decrypt(key)?));
    }

    if card.exp_month.is_some() || card.exp_year.is_some() {
        fields.push(Field::card_expiration(
            card.exp_month.map(|exp| exp.decrypt(key)).transpose()?,
            card.exp_year.map(|exp| exp.decrypt(key)).transpose()?,
        ));
    }

    if let Some(code) = card.code {
        fields.push(Field::card_code(code.decrypt(key)?));
    }

    Ok(icon.unwrap_or(Icon::Glyph(icons::Glyph::Card)))
}

fn process_identity(
    identity: data::Identity,
    key: &SymmetricKey,
    fields: &mut Vec<Field>,
) -> anyhow::Result<Icon> {
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

    Ok(Icon::Glyph(icons::Glyph::Identity))
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

use cipher_set::CipherSet;
/// Newtype over a `Box<[Cipher]>` for type-safety.
mod cipher_set {
    pub(super) struct CipherSet(Box<[Cipher]>);

    impl CipherSet {
        pub(super) const fn from_boxed_slice(slice: Box<[Cipher]>) -> Self {
            Self(slice)
        }
        pub(crate) fn enumerated(&self) -> impl Iterator<Item = (Index, &Cipher)> {
            self.0
                .iter()
                .enumerate()
                .map(|(i, cipher)| (Index(i), cipher))
        }
        pub(crate) fn filter(&self, filter: &CipherFilter) -> Option<Index> {
            // TODO: Maybe parallelize this
            Some(Index(match filter {
                &CipherFilter::Uuid(uuid) => {
                    let uuid = Uuid::from_bytes(uuid);
                    self.0.iter().position(|cipher| cipher.id == uuid)?
                }
                CipherFilter::Name(name) => {
                    self.0.iter().position(|cipher| *cipher.name == **name)?
                }
            }))
        }
    }

    impl ops::Index<Index> for CipherSet {
        type Output = Cipher;

        fn index(&self, index: Index) -> &Self::Output {
            &self.0[index.0]
        }
    }

    impl ops::IndexMut<Index> for CipherSet {
        fn index_mut(&mut self, index: Index) -> &mut Self::Output {
            &mut self.0[index.0]
        }
    }

    impl<'cipher_set> IntoIterator for &'cipher_set CipherSet {
        type Item = &'cipher_set Cipher;
        type IntoIter = slice::Iter<'cipher_set, Cipher>;
        fn into_iter(self) -> Self::IntoIter {
            self.0.iter()
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct Index(usize);

    use super::Cipher;
    use core::slice;
    use rofi_bw_common::ipc::CipherFilter;
    use std::ops;
    use uuid::Uuid;
}

use cipher_type_list::CipherTypeList;
mod cipher_type_list {
    #[derive(Default)]
    pub(crate) struct CipherTypeList<T>([T; 4]);

    impl<T> ops::Index<CipherType> for CipherTypeList<T> {
        type Output = T;
        fn index(&self, index: CipherType) -> &Self::Output {
            &self.0[index as usize]
        }
    }

    impl<T> ops::IndexMut<CipherType> for CipherTypeList<T> {
        fn index_mut(&mut self, index: CipherType) -> &mut Self::Output {
            &mut self.0[index as usize]
        }
    }

    use super::CipherType;
    use std::ops;
}

struct Cipher {
    id: Uuid,
    /// Used to sort ciphers into type buckets.
    r#type: CipherType,
    deleted: bool,
    favourite: bool,
    name: String,
    icon: Icon,
    reprompt: bool,
    fields: Vec<Field>,
    default_copy: Option<usize>,
}

impl Cipher {
    const fn safe_uninit() -> Self {
        Self {
            id: Uuid::nil(),
            r#type: CipherType::Login,
            deleted: false,
            favourite: false,
            name: String::new(),
            icon: Icon::Glyph(icons::Glyph::Login),
            reprompt: false,
            fields: Vec::new(),
            default_copy: None,
        }
    }
}

struct Field {
    display: Cow<'static, str>,
    icon: Icon,
    action: Option<Action>,
}

impl Field {
    fn username(username: String) -> Self {
        Self::shown("Username", "username", username, icons::Glyph::User)
    }
    fn password(password: String) -> Self {
        Self::hidden("Password", "password", password, icons::Glyph::Key)
    }
    fn uri(uri: String) -> Self {
        Self::shown("Uri", "URI", uri, icons::Glyph::Chain)
    }
    fn cardholder_name(name: String) -> Self {
        let icon = icons::Glyph::User;
        Self::shown("Cardholder name", "cardholder name", name, icon)
    }
    fn card_brand(brand: String) -> Self {
        let icon = Icon::card(&*brand).unwrap_or(Icon::Glyph(icons::Glyph::Card));
        Self::shown("Brand", "brand", brand, icon)
    }
    fn card_number(number: String) -> Self {
        Self::hidden("Number", "number", number, icons::Glyph::Hash)
    }
    fn card_expiration(month: Option<String>, year: Option<String>) -> Self {
        // TODO: Internationalize this?
        let expiration = format!(
            "{} / {}",
            month.as_deref().unwrap_or("__"),
            year.as_deref().unwrap_or("____"),
        );
        Self::shown("Expiration", "expiration", expiration, icons::Glyph::Clock)
    }
    fn card_code(code: String) -> Self {
        let icon = icons::Glyph::Padlock;
        Self::hidden("Security code", "security code", code, icon)
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
        Self::shown("Identity name", "name", name, icons::Glyph::Identity)
    }
    fn identity_username(username: String) -> Self {
        Self::shown("Username", "username", username, icons::Glyph::User)
    }
    fn identity_company(company: String) -> Self {
        Self::shown("Company", "company", company, icons::Glyph::Briefcase)
    }
    fn identity_ssn(ssn: String) -> Self {
        // TODO: Internationalize
        Self::shown(
            "National Insurance number",
            "national insurance number",
            ssn,
            icons::Glyph::Hash,
        )
    }
    fn identity_passport_number(number: String) -> Self {
        let icon = icons::Glyph::Login;
        Self::shown("Passport number", "passport number", number, icon)
    }
    fn identity_licence_number(licence_number: String) -> Self {
        let icon = icons::Glyph::Identity;
        Self::shown("Licence number", "licence number", licence_number, icon)
    }
    fn identity_email(email: String) -> Self {
        Self::shown("Email", "email", email, icons::Glyph::Mail)
    }
    fn identity_phone(phone: String) -> Self {
        Self::shown("Phone", "phone", phone, icons::Glyph::Mobile)
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
            icon: Icon::Glyph(icons::Glyph::List),
            action: Some(Action::Copy {
                name: Cow::Borrowed("address"),
                data,
                hidden: false,
            }),
        }
    }
    fn notes(notes: String) -> Self {
        Self {
            // TODO: Note preview
            display: Cow::Borrowed("Notes"),
            icon: Icon::Glyph(icons::Glyph::SecureNote),
            action: Some(Action::Copy {
                name: Cow::Borrowed("note"),
                data: notes,
                hidden: false,
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
            FieldValue::Boolean(v) => format!("{display_name}: {v}"),
            FieldValue::Linked(v) => format!("{display_name} → {v}"),
        });

        let icon = Icon::Glyph(match &value {
            FieldValue::Text(_) => icons::Glyph::Pencil,
            FieldValue::Hidden(_) => icons::Glyph::EyeSlash,
            FieldValue::Boolean(false) => icons::Glyph::Square,
            FieldValue::Boolean(true) => icons::Glyph::SquareCheck,
            FieldValue::Linked(_) => icons::Glyph::Chain,
        });

        let name = name.map(Cow::Owned);

        let action = match value {
            FieldValue::Text(text) => Some(Action::Copy {
                name: name.unwrap_or(Cow::Borrowed("text field")),
                data: text.unwrap_or_default(),
                hidden: false,
            }),
            FieldValue::Hidden(hidden) => Some(Action::Copy {
                name: name.unwrap_or(Cow::Borrowed("hidden field")),
                data: hidden.unwrap_or_default(),
                hidden: true,
            }),
            FieldValue::Boolean(v) => Some(Action::Copy {
                name: name.unwrap_or(Cow::Borrowed("boolean field")),
                data: v.to_string(),
                hidden: false,
            }),
            FieldValue::Linked(to) => Some(Action::Link { to: to.as_str() }),
        };

        Self {
            display,
            icon,
            action,
        }
    }

    fn shown(title: &'static str, name: &'static str, data: String, icon: impl Into<Icon>) -> Self {
        Self {
            display: Cow::Owned(format!("{title}: {data}")),
            icon: icon.into(),
            action: Some(Action::Copy {
                name: Cow::Borrowed(name),
                data,
                hidden: false,
            }),
        }
    }

    fn hidden(
        title: &'static str,
        name: &'static str,
        data: String,
        icon: impl Into<Icon>,
    ) -> Self {
        Self {
            display: Cow::Borrowed(title),
            icon: icon.into(),
            action: Some(Action::Copy {
                name: Cow::Borrowed(name),
                data,
                hidden: true,
            }),
        }
    }
}

enum FieldValue {
    Text(Option<String>),
    Hidden(Option<String>),
    Boolean(bool),
    Linked(data::Linked),
}

enum Action {
    Copy {
        name: Cow<'static, str>,
        data: String,
        /// Used to check whether a reprompt is necessary.
        hidden: bool,
    },
    Link {
        to: &'static str,
    },
}

use crate::data;
use crate::data::CipherData;
use crate::data::Data;
use crate::icons;
use crate::parallel_try_fill;
use crate::Icon;
use crate::Icons;
use crate::SymmetricKey;
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use rofi_bw_common::ipc;
use rofi_bw_common::CipherList;
use rofi_bw_common::CipherType;
use rofi_bw_common::MasterKey;
use rofi_mode::cairo;
use std::borrow::Cow;
use std::sync::Arc;
use url::Url;
use uuid::Uuid;
