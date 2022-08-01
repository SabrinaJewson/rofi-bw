#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Data {
    pub(crate) profile: Profile,
    pub(crate) folders: Vec<Folder>,
    pub(crate) ciphers: Vec<Cipher>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Profile {
    // pub(crate) id: Uuid,
    // pub(crate) name: Option<String>,
    // pub(crate) email: String,
    // pub(crate) email_verified: bool,
    // pub(crate) premium: bool,
    // pub(crate) premium_from_organization: bool,
    // pub(crate) master_password_hint: Option<String>,
    // pub(crate) culture: String,
    // pub(crate) two_factor_enabled: bool,
    pub(crate) key: CipherString<SymmetricKey>,
    // TODO: better types? I don’t know what this does
    // pub(crate) private_key: Option<CipherString<Vec<u8>>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Folder {
    pub(crate) id: Uuid,
    pub(crate) name: CipherString<String>,
    // #[serde(with = "time::serde::rfc3339")]
    // pub(crate) revision_date: OffsetDateTime,
}

#[derive(Debug)]
pub(crate) struct Cipher {
    pub(crate) id: Uuid,
    pub(crate) folder_id: Option<Uuid>,
    pub(crate) name: CipherString<String>,
    // pub(crate) revision_date: OffsetDateTime,
    pub(crate) deleted_date: Option<OffsetDateTime>,
    pub(crate) reprompt: bool,
    // pub(crate) edit: bool,
    pub(crate) favourite: bool,
    pub(crate) data: CipherData,
    pub(crate) notes: Option<CipherString<String>>,
    pub(crate) fields: Option<Vec<Field>>,
}

impl<'de> Deserialize<'de> for Cipher {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Inner {
            id: Uuid,
            folder_id: Option<Uuid>,
            name: CipherString<String>,
            // #[serde(with = "time::serde::rfc3339")]
            // revision_date: OffsetDateTime,
            #[serde(with = "time::serde::rfc3339::option")]
            deleted_date: Option<OffsetDateTime>,
            reprompt: u8,
            // edit: bool,
            favorite: bool,
            notes: Option<CipherString<String>>,
            fields: Option<Vec<Field>>,

            r#type: u32,
            login: Option<Login>,
            // this doesn’t contain anything interesting
            secure_note: Option<de::IgnoredAny>,
            card: Option<Card>,
            identity: Option<Identity>,
        }
        let inner = Inner::deserialize(deserializer)?;
        Ok(Self {
            id: inner.id,
            folder_id: inner.folder_id,
            name: inner.name,
            // revision_date: inner.revision_date,
            deleted_date: inner.deleted_date,
            reprompt: inner.reprompt != 0,
            // edit: inner.edit,
            favourite: inner.favorite,
            notes: inner.notes,
            fields: inner.fields,
            data: None
                .or_else(|| inner.login.map(CipherData::Login))
                .or_else(|| inner.secure_note.map(|_| CipherData::SecureNote))
                .or_else(|| inner.card.map(CipherData::Card))
                .or_else(|| inner.identity.map(CipherData::Identity))
                .ok_or_else(|| {
                    de::Error::custom(format_args!("unknown card type {}", inner.r#type))
                })?,
        })
    }
}

#[derive(Debug)]
pub(crate) enum CipherData {
    Login(Login),
    SecureNote,
    Card(Card),
    Identity(Identity),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Login {
    pub(crate) uri: Option<CipherString<String>>,
    pub(crate) uris: Option<Vec<Uri>>,
    pub(crate) username: Option<CipherString<String>>,
    pub(crate) password: Option<CipherString<String>>,
    // #[serde(with = "time::serde::rfc3339::option")]
    // pub(crate) password_revision_date: Option<OffsetDateTime>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Card {
    pub(crate) cardholder_name: Option<CipherString<String>>,
    pub(crate) brand: Option<CipherString<String>>,
    pub(crate) number: Option<CipherString<String>>,
    pub(crate) exp_month: Option<CipherString<String>>,
    pub(crate) exp_year: Option<CipherString<String>>,
    pub(crate) code: Option<CipherString<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Identity {
    pub(crate) title: Option<CipherString<String>>,
    pub(crate) first_name: Option<CipherString<String>>,
    pub(crate) middle_name: Option<CipherString<String>>,
    pub(crate) last_name: Option<CipherString<String>>,
    pub(crate) address1: Option<CipherString<String>>,
    pub(crate) address2: Option<CipherString<String>>,
    pub(crate) address3: Option<CipherString<String>>,
    pub(crate) city: Option<CipherString<String>>,
    pub(crate) state: Option<CipherString<String>>,
    pub(crate) postal_code: Option<CipherString<String>>,
    pub(crate) country: Option<CipherString<String>>,
    pub(crate) company: Option<CipherString<String>>,
    pub(crate) email: Option<CipherString<String>>,
    pub(crate) phone: Option<CipherString<String>>,
    pub(crate) ssn: Option<CipherString<String>>,
    pub(crate) username: Option<CipherString<String>>,
    pub(crate) passport_number: Option<CipherString<String>>,
    pub(crate) license_number: Option<CipherString<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Uri {
    pub(crate) uri: CipherString<String>,
    #[serde(rename = "match")]
    // Dead code, but there’s enough of it that I want to make sure it always compiles
    #[allow(dead_code)]
    pub(crate) match_type: Option<UriMatchType>,
}

#[derive(Debug)]
pub(crate) enum UriMatchType {
    Domain = 0,
    Host = 1,
    StartsWith = 2,
    Exact = 3,
    RegularExpression = 4,
    Never = 5,
}

impl<'de> Deserialize<'de> for UriMatchType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = UriMatchType;
            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("a URI match type")
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(match v {
                    0 => UriMatchType::Domain,
                    1 => UriMatchType::Host,
                    2 => UriMatchType::StartsWith,
                    3 => UriMatchType::Exact,
                    4 => UriMatchType::RegularExpression,
                    5 => UriMatchType::Never,
                    _ => return Err(de::Error::invalid_value(de::Unexpected::Unsigned(v), &self)),
                })
            }
        }
        deserializer.deserialize_u64(Visitor)
    }
}

#[derive(Debug)]
pub(crate) struct Field {
    pub(crate) name: Option<CipherString<String>>,
    pub(crate) value: FieldValue,
}

#[derive(Debug)]
pub(crate) enum FieldValue {
    Text(Option<CipherString<String>>),
    Hidden(Option<CipherString<String>>),
    Boolean(CipherString<bool>),
    Linked(Linked),
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Field;
            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("a field")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                enum Key {
                    Name,
                    Type,
                    Value,
                    LinkedId,
                }

                // type, name, value, linkedId
                let mut name: Option<Option<CipherString<String>>> = None;
                let mut r#type: Option<u64> = None;
                let mut value: Option<Option<cipher_string::Untyped>> = None;
                let mut linked: Option<Option<Linked>> = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Key::Name => {
                            if name.is_some() {
                                return Err(de::Error::duplicate_field("name"));
                            }
                            name = Some(map.next_value()?);
                        }
                        Key::Type => {
                            if r#type.is_some() {
                                return Err(de::Error::duplicate_field("type"));
                            }
                            r#type = Some(map.next_value()?);
                        }
                        Key::Value => {
                            if value.is_some() {
                                return Err(de::Error::duplicate_field("value"));
                            }
                            value = Some(map.next_value()?);
                        }
                        Key::LinkedId => {
                            if linked.is_some() {
                                return Err(de::Error::duplicate_field("linkedId"));
                            }
                            linked = Some(map.next_value()?);
                        }
                    }
                }

                let name = name.ok_or_else(|| de::Error::missing_field("name"))?;
                let r#type = r#type.ok_or_else(|| de::Error::missing_field("type"))?;
                let value = value.ok_or_else(|| de::Error::missing_field("value"))?;
                let linked_id = linked.ok_or_else(|| de::Error::missing_field("linkedId"))?;

                let value = match r#type {
                    0 => FieldValue::Text(value.map(CipherString::from)),
                    1 => FieldValue::Hidden(value.map(CipherString::from)),
                    2 => FieldValue::Boolean(CipherString::from(value.ok_or_else(|| {
                        de::Error::invalid_type(de::Unexpected::Unit, &"an encrypted boolean")
                    })?)),
                    3 => FieldValue::Linked(linked_id.ok_or_else(|| {
                        de::Error::invalid_type(de::Unexpected::Unit, &"a linked ID")
                    })?),
                    _ => {
                        return Err(de::Error::invalid_value(
                            de::Unexpected::Unsigned(r#type),
                            &"a field type",
                        ))
                    }
                };

                Ok(Field { name, value })
            }
        }
        let fields = &["name", "type", "value", "linkedId"];
        deserializer.deserialize_struct("Field", fields, Visitor)
    }
}

macro_rules! define_linked {
    (
        $($cipher_type:ident($linked_type:ident) {
            $($field:ident = $value:expr,)*
        },)*
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum Linked {
            $($cipher_type($linked_type),)*
        }

        $(
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(crate) enum $linked_type {
                $($field = $value,)*
            }

            impl $linked_type {
                pub(crate) fn as_str(self) -> &'static str {
                    match self {
                        $(Self::$field => stringify!($field),)*
                    }
                }
            }

            impl Display for $linked_type {
                fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                    Display::fmt(self.as_str(), f)
                }
            }
        )*

        impl Linked {
            pub(crate) fn from_id(id: u64) -> Option<Self> {
                match id {
                    $($($value => Some(Self::$cipher_type($linked_type::$field)),)*)*
                    _ => None,
                }
            }

            pub(crate) fn as_str(self) -> &'static str {
                match self {
                    $(Self::$cipher_type(linked) => linked.as_str(),)*
                }
            }
        }

        impl Display for Linked {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                Display::fmt(self.as_str(), f)
            }
        }
    };
}

// From:
// https://github.com/bitwarden/clients/blob/master/libs/common/src/enums/linkedIdType.ts
define_linked! {
    Login(LoginLinked) {
        Username = 100,
        Password = 101,
    },
    Card(CardLinked) {
        CardholderName = 300,
        ExpMonth = 301,
        ExpYear = 302,
        Code = 303,
        Brand = 304,
        Number = 305,
    },
    Identity(IdentityLinked) {
        Title = 400,
        MiddleName = 401,
        Address1 = 402,
        Address2 = 403,
        Address3 = 404,
        City = 405,
        State = 406,
        PostalCode = 407,
        Country = 408,
        Company = 409,
        Email = 410,
        Phone = 411,
        Ssn = 412,
        Username = 413,
        PassportNumber = 414,
        LicenseNumber = 415,
        FirstName = 416,
        LastName = 417,
        FullName = 418,
    },
}

impl<'de> Deserialize<'de> for Linked {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Linked;
            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("a linked ID")
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Linked::from_id(v)
                    .ok_or_else(|| de::Error::invalid_value(de::Unexpected::Unsigned(v), &self))
            }
        }
        deserializer.deserialize_u64(Visitor)
    }
}

use crate::cipher_string;
use crate::symmetric_key::SymmetricKey;
use crate::CipherString;
use serde::de;
use serde::Deserialize;
use serde::Deserializer;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use time::OffsetDateTime;
use uuid::Uuid;
