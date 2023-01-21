pub(crate) struct Initialized {
    state: State,
    icons: Icons,
    // Currently unused, but may be useful in future
    error_message: String,
}

impl Initialized {
    pub(crate) fn new(
        master_key: &MasterKey,
        data: Data,
        history: History<ipc::View>,
    ) -> anyhow::Result<Self> {
        let mut icons = Icons::new()?;

        let state = State::new(master_key, data, history)?;

        for cipher in &*state.ciphers {
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
        s.push_str(match *self.state.history.current() {
            View::List(list) => list.description(),
            View::Folder(i) => &self.state.folders[i].name,
            View::Cipher(i) => &self.state.ciphers[i].name,
        });
        s.push_str("\n");

        if !self.error_message.is_empty() {
            s.push_str(&self.error_message);
        }
    }

    pub(crate) fn entries(&self) -> usize {
        match self.state.viewing() {
            Viewing::CipherList(list) => list.len(),
            Viewing::Folders(folders) => folders.len(),
            Viewing::Cipher(cipher) => cipher.fields.len(),
        }
    }

    pub(crate) fn entry_content(&self, line: usize) -> &str {
        match self.state.viewing() {
            Viewing::CipherList(list) => &self.state.ciphers[list[line]].name,
            Viewing::Folders(folders) => &folders[typed_slice::Index::from_raw(line)].name,
            Viewing::Cipher(cipher) => &cipher.fields[line].display,
        }
    }

    pub(crate) fn entry_icon(&mut self, line: usize, height: u32) -> Option<cairo::Surface> {
        let icon = match self.state.viewing() {
            Viewing::CipherList(list) => &self.state.ciphers[list[line]].icon,
            Viewing::Folders(_) => &Icon::Glyph(icons::Glyph::Folder),
            Viewing::Cipher(cipher) => &cipher.fields[line].icon,
        };
        self.icons.surface(icon, height)
    }

    pub(crate) fn show(&mut self, list: List) {
        self.state.history.push(View::List(list));
    }

    pub(crate) fn parent(&mut self) {
        let parent = match *self.state.history.current() {
            View::List(List::Trash) => View::List(List::Trash),
            View::List(List::All | List::Favourites | List::TypeBucket(_)) => View::List(List::All),
            View::List(List::Folders) | View::Folder(_) => View::List(List::Folders),
            View::Cipher(i) => {
                let folder_id = self.state.ciphers[i].folder_id;
                View::Folder(self.state.folder_map[&folder_id])
            }
        };
        self.state.history.push(parent);
    }

    pub(crate) fn navigate(&mut self, navigate: Navigate) {
        match navigate {
            Navigate::Back => self.state.history.back(),
            Navigate::Forward => self.state.history.forward(),
        }
    }

    pub(crate) fn ok_alt(&mut self, line: usize, input: &mut rofi_mode::String) {
        match self.state.viewing() {
            Viewing::CipherList(list) => {
                input.clear();
                self.state.history.push(View::Cipher(list[line]));
            }
            Viewing::Folders(_) => {
                input.clear();
                self.state
                    .history
                    .push(View::Folder(typed_slice::Index::from_raw(line)));
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
                let cipher = &self.state.ciphers[list[line]];
                match cipher.default_copy {
                    Some(default_copy) => (cipher, default_copy),
                    None => {
                        input.clear();
                        self.state.history.push(View::Cipher(list[line]));
                        return None;
                    }
                }
            }
            Viewing::Folders(_) => {
                input.clear();
                self.state
                    .history
                    .push(View::Folder(typed_slice::Index::from_raw(line)));
                return None;
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
                        history: self.ipc_state(),
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

    pub(crate) fn history(&self) -> &History<impl PartialEq> {
        &self.state.history
    }

    pub(crate) fn ipc_state(&self) -> History<ipc::View> {
        self.state.history.ref_map(|view| match *view {
            View::List(list) => ipc::View::List(list),
            View::Folder(i) => {
                let uuid = self.state.folders[i].id;
                if let Some(uuid) = uuid {
                    ipc::View::Folder(ipc::Filter::Uuid(uuid.into_bytes()))
                } else {
                    ipc::View::NoFolder
                }
            }
            View::Cipher(i) => {
                let uuid = self.state.ciphers[i].id;
                ipc::View::Cipher(ipc::Filter::Uuid(uuid.into_bytes()))
            }
        })
    }
}

struct State {
    history: History<View>,
    ciphers: Box<TypedSlice<Cipher>>,
    all: Vec<typed_slice::Index<Cipher>>,
    trash: Vec<typed_slice::Index<Cipher>>,
    favourites: Vec<typed_slice::Index<Cipher>>,
    type_buckets: CipherTypeList<Vec<typed_slice::Index<Cipher>>>,
    folders: Box<TypedSlice<Folder>>,
    folder_map: FolderMap,
}

type FolderMap = HashMap<Option<Uuid>, typed_slice::Index<Folder>>;

#[derive(Clone, Copy, PartialEq)]
enum View {
    List(List),
    Folder(typed_slice::Index<Folder>),
    Cipher(typed_slice::Index<Cipher>),
}

impl State {
    pub(crate) fn new(
        master_key: &MasterKey,
        data: Data,
        history: History<ipc::View>,
    ) -> anyhow::Result<Self> {
        let key = data.profile.key.decrypt(master_key)?;

        let collator = Collator::default_locale()?;

        let (folders_result, ciphers_result) = rayon::join(
            || process_folders(data.folders, &key, &collator),
            || process_ciphers(data.ciphers, &key, &collator),
        );
        let (mut folders, folder_map) = folders_result?;
        let ciphers = ciphers_result?;

        let mut all = Vec::new();
        let mut trash = Vec::new();
        let mut favourites = Vec::new();
        let mut type_buckets = <CipherTypeList<Vec<_>>>::default();
        for (i, cipher) in ciphers.enumerated() {
            if cipher.deleted {
                trash.push(i);
                continue;
            }

            if cipher.favourite {
                favourites.push(i);
            }

            all.push(i);

            type_buckets[cipher.r#type].push(i);

            let &folder = folder_map.get(&cipher.folder_id).with_context(|| {
                format!("Item {} is contained in non-existent folder", cipher.name)
            })?;
            folders[folder].contents.push(i);
        }

        Ok(Self {
            history: history.map(|view| match view {
                ipc::View::List(list) => View::List(list),
                ipc::View::NoFolder => View::Folder(folders.last_index()),
                ipc::View::Folder(filter) => {
                    let index = match filter {
                        ipc::Filter::Uuid(uuid) => {
                            let uuid = Uuid::from_bytes(uuid);
                            folders.position(|folder| folder.id == Some(uuid))
                        }
                        ipc::Filter::Name(name) => folders.position(|folder| folder.name == name),
                    };

                    index.map_or(View::List(List::All), View::Folder)
                }
                ipc::View::Cipher(filter) => {
                    let index = match filter {
                        ipc::Filter::Uuid(uuid) => {
                            let uuid = Uuid::from_bytes(uuid);
                            ciphers.position(|cipher| cipher.id == uuid)
                        }
                        ipc::Filter::Name(name) => ciphers.position(|cipher| cipher.name == name),
                    };

                    index.map_or(View::List(List::All), View::Cipher)
                }
            }),
            ciphers,
            all,
            trash,
            favourites,
            type_buckets,
            folders,
            folder_map,
        })
    }

    pub(crate) fn viewing(&self) -> Viewing<'_> {
        match *self.history.current() {
            View::List(list) => match list {
                List::All => Viewing::CipherList(&self.all),
                List::Trash => Viewing::CipherList(&self.trash),
                List::Favourites => Viewing::CipherList(&self.favourites),
                List::TypeBucket(cipher_type) => {
                    Viewing::CipherList(&self.type_buckets[cipher_type])
                }
                List::Folders => Viewing::Folders(&self.folders),
            },
            View::Folder(i) => Viewing::CipherList(&self.folders[i].contents),
            View::Cipher(i) => Viewing::Cipher(&self.ciphers[i]),
        }
    }
}

enum Viewing<'a> {
    CipherList(&'a [typed_slice::Index<Cipher>]),
    Folders(&'a TypedSlice<Folder>),
    Cipher(&'a Cipher),
}

fn process_folders(
    folders: Vec<data::Folder>,
    key: &SymmetricKey,
    collator: &Collator,
) -> anyhow::Result<(Box<TypedSlice<Folder>>, FolderMap)> {
    let mut processed = Vec::with_capacity(folders.len() + 1);

    for folder in folders {
        processed.push(process_folder(folder, key)?);
    }

    try_sort::unstable_by(&mut processed, |a, b| -> anyhow::Result<_> {
        Ok(collator
            .strcoll_utf8(&a.name, &b.name)?
            .then_with(|| a.id.cmp(&b.id)))
    })?;

    processed.push(Folder {
        id: None,
        name: "No folder".to_owned(),
        contents: Vec::new(),
    });

    let processed = TypedSlice::from_boxed_slice(processed.into_boxed_slice());

    let map = processed
        .enumerated()
        .map(|(i, folder)| (folder.id, i))
        .collect::<HashMap<Option<Uuid>, typed_slice::Index<Folder>>>();

    Ok((processed, map))
}

fn process_ciphers(
    ciphers: Vec<data::Cipher>,
    key: &SymmetricKey,
    collator: &Collator,
) -> anyhow::Result<Box<TypedSlice<Cipher>>> {
    let mut processed = (0..ciphers.len())
        .map(|_| Cipher::safe_uninit())
        .collect::<Box<[_]>>();

    ciphers
        .into_par_iter()
        .zip_eq(&mut *processed)
        .try_for_each(|(cipher, out)| {
            *out = process_cipher(cipher, key)?;
            anyhow::Ok(())
        })?;

    try_sort::unstable_by(&mut processed, |a, b| -> anyhow::Result<_> {
        Ok(collator
            .strcoll_utf8(&a.name, &b.name)?
            .then_with(|| a.id.cmp(&b.id)))
    })?;

    Ok(TypedSlice::from_boxed_slice(processed))
}

fn process_folder(folder: data::Folder, key: &SymmetricKey) -> anyhow::Result<Folder> {
    Ok(Folder {
        id: Some(folder.id),
        name: folder.name.decrypt(key)?,
        contents: Vec::new(),
    })
}

struct Folder {
    /// None for the “No folder” folder
    id: Option<Uuid>,
    name: String,
    contents: Vec<typed_slice::Index<Cipher>>,
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
        folder_id: cipher.folder_id,
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
        icon = Icon::card(&brand);
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
    let url = Url::parse(url).ok().or_else(|| {
        if url.starts_with("http://") || url.starts_with("https://") || !url.contains(".") {
            return None;
        }
        Url::parse(&format!("http://{url}")).ok()
    })?;

    match url.host()? {
        url::Host::Domain(domain) => Some(<Arc<str>>::from(domain)),
        _ => None,
    }
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
    folder_id: Option<Uuid>,
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
            folder_id: None,
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
        let icon = Icon::card(&brand).unwrap_or(Icon::Glyph(icons::Glyph::Card));
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
                name.push_str(&part);
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

use typed_slice::TypedSlice;
/// Newtype over a `[T]` featuring `T`-dependent index types.
mod typed_slice {
    #[repr(transparent)]
    pub(super) struct TypedSlice<T>([T]);

    impl<T> TypedSlice<T> {
        pub(super) fn from_boxed_slice(slice: Box<[T]>) -> Box<Self> {
            let ptr = Box::into_raw(slice);
            let ptr = ptr as *mut Self;
            unsafe { Box::from_raw(ptr) }
        }
        pub(crate) const fn len(&self) -> usize {
            self.0.len()
        }
        pub(crate) fn iter(&self) -> slice::Iter<'_, T> {
            self.0.iter()
        }
        pub(crate) fn iter_mut(&mut self) -> slice::IterMut<'_, T> {
            self.0.iter_mut()
        }
        pub(crate) fn enumerated(&self) -> impl Iterator<Item = (Index<T>, &T)> {
            self.0
                .iter()
                .enumerate()
                .map(|(i, value)| (Index::from_raw(i), value))
        }
        pub(crate) fn position<F>(&self, f: F) -> Option<Index<T>>
        where
            F: FnMut(&T) -> bool,
        {
            self.0.iter().position(f).map(Index::from_raw)
        }
        pub(crate) fn last_index(&self) -> Index<T> {
            Index::from_raw(self.0.len() - 1)
        }
    }

    impl<T> ops::Index<Index<T>> for TypedSlice<T> {
        type Output = T;

        fn index(&self, index: Index<T>) -> &Self::Output {
            &self.0[index.raw]
        }
    }

    impl<T> ops::IndexMut<Index<T>> for TypedSlice<T> {
        fn index_mut(&mut self, index: Index<T>) -> &mut Self::Output {
            &mut self.0[index.raw]
        }
    }

    impl<'list, T> IntoIterator for &'list TypedSlice<T> {
        type Item = &'list T;
        type IntoIter = slice::Iter<'list, T>;
        fn into_iter(self) -> Self::IntoIter {
            self.iter()
        }
    }

    impl<'list, T> IntoIterator for &'list mut TypedSlice<T> {
        type Item = &'list mut T;
        type IntoIter = slice::IterMut<'list, T>;
        fn into_iter(self) -> Self::IntoIter {
            self.iter_mut()
        }
    }

    #[derive(Debug)]
    pub(crate) struct Index<T> {
        raw: usize,
        phantom: PhantomData<fn() -> T>,
    }

    impl<T> Index<T> {
        pub(crate) fn from_raw(raw: usize) -> Self {
            Self {
                raw,
                phantom: PhantomData,
            }
        }
    }

    impl<T> Clone for Index<T> {
        fn clone(&self) -> Self {
            *self
        }
    }

    impl<T> Copy for Index<T> {}

    impl<T> PartialEq for Index<T> {
        fn eq(&self, other: &Self) -> bool {
            self.raw == other.raw
        }
    }

    use core::slice;
    use std::marker::PhantomData;
    use std::ops;
}

use collator::Collator;
mod collator {
    pub(crate) struct Collator {
        rep: ptr::NonNull<rust_icu_sys::UCollator>,
    }

    unsafe impl Send for Collator {}

    // SAFETY: ICU APIs are thread-safe
    unsafe impl Sync for Collator {}

    const _: () = {
        // TODO: Don’t import this: https://github.com/google/rust_icu/pull/251
        #[allow(clippy::wildcard_imports)]
        use rust_icu_sys::*;
        // TODO: Don’t import this: https://github.com/google/rust_icu/pull/252
        use rust_icu_sys::versioned_function;
        rust_icu_common::simple_drop_impl!(Collator, ucol_close);
    };

    impl Collator {
        pub(crate) fn default_locale() -> anyhow::Result<Self> {
            let mut status = rust_icu_common::Error::OK_CODE;
            let rep = unsafe {
                // TODO: Don’t import this: https://github.com/google/rust_icu/pull/251
                #[allow(clippy::wildcard_imports)]
                use rust_icu_sys::*;
                versioned_function!(ucol_open)(ptr::null(), &mut status)
            };
            rust_icu_common::Error::ok_or_warning(status)
                .context("failed to open Unicode collator")?;

            Ok(Self {
                rep: ptr::NonNull::new(rep).unwrap(),
            })
        }

        pub(crate) fn strcoll_utf8(&self, a: &str, b: &str) -> anyhow::Result<cmp::Ordering> {
            self.strcoll_utf8_inner(a, b)
                .context("failed to compare two strings")
        }

        fn strcoll_utf8_inner(&self, a: &str, b: &str) -> anyhow::Result<cmp::Ordering> {
            let a_len = i32::try_from(a.len()).context("a string is too long")?;
            let b_len = i32::try_from(b.len()).context("b string is too long")?;

            let mut status = rust_icu_common::Error::OK_CODE;
            let res = unsafe {
                #[allow(clippy::wildcard_imports)]
                use rust_icu_sys::*;
                versioned_function!(ucol_strcollUTF8)(
                    self.rep.as_ptr(),
                    a.as_ptr().cast(),
                    a_len,
                    b.as_ptr().cast(),
                    b_len,
                    &mut status,
                )
            };
            rust_icu_common::Error::ok_or_warning(status)?;
            Ok(match res {
                rust_icu_sys::UCollationResult::UCOL_LESS => cmp::Ordering::Less,
                rust_icu_sys::UCollationResult::UCOL_EQUAL => cmp::Ordering::Equal,
                rust_icu_sys::UCollationResult::UCOL_GREATER => cmp::Ordering::Greater,
            })
        }
    }

    use anyhow::Context as _;
    use std::cmp;
    use std::ptr;
}

mod try_sort {
    pub(crate) fn unstable_by<T, E, F>(slice: &mut [T], mut compare: F) -> Result<(), E>
    where
        F: FnMut(&T, &T) -> Result<cmp::Ordering, E>,
    {
        let mut res = Ok(());
        slice.sort_unstable_by(|a, b| {
            if res.is_err() {
                return cmp::Ordering::Equal;
            }
            match compare(a, b) {
                Ok(ordering) => ordering,
                Err(e) => {
                    res = Err(e);
                    cmp::Ordering::Equal
                }
            }
        });
        res
    }

    use std::cmp;
}

use crate::data;
use crate::data::CipherData;
use crate::data::Data;
use crate::icons;
use crate::Icon;
use crate::Icons;
use crate::SymmetricKey;
use anyhow::Context as _;
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use rayon::prelude::IndexedParallelIterator;
use rofi_bw_common::ipc;
use rofi_bw_common::menu_keybinds::Navigate;
use rofi_bw_common::CipherType;
use rofi_bw_common::List;
use rofi_bw_common::MasterKey;
use rofi_bw_util::History;
use rofi_mode::cairo;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use url::Url;
use uuid::Uuid;
