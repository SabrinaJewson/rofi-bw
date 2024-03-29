pub(crate) use prelogin::prelogin;
pub(crate) use prelogin::Pbkdf2Algorithm;
pub(crate) use prelogin::Prelogin;
pub(crate) mod prelogin {
    pub(crate) fn prelogin(http: &ureq::Agent, email: &str) -> Result<Prelogin, Error> {
        inner(http, email).map_err(|kind| Error {
            kind,
            email: email.into(),
        })
    }

    fn inner(http: &ureq::Agent, email: &str) -> Result<Prelogin, ErrorKind> {
        #[derive(Serialize)]
        struct Body<'email> {
            email: &'email str,
        }

        http.post("https://vault.bitwarden.com/api/accounts/prelogin")
            .send_json(Body { email })
            .map_err(|e| ErrorKind::Http(Box::new(e)))?
            .into_json()
            .map_err(ErrorKind::Body)
    }

    #[derive(Debug)]
    pub(crate) struct Error {
        kind: ErrorKind,
        email: Box<str>,
    }

    impl Display for Error {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(
                f,
                "failed to download prelogin data for account {}",
                self.email
            )
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn 'static + std::error::Error)> {
            match &self.kind {
                ErrorKind::Http(e) => Some(e),
                ErrorKind::Body(e) => Some(e),
            }
        }
    }

    #[derive(Debug)]
    pub(crate) enum ErrorKind {
        Http(Box<ureq::Error>),
        Body(io::Error),
    }

    #[derive(Debug)]
    pub(crate) enum Prelogin {
        Pbkdf2 {
            algorithm: Pbkdf2Algorithm,
            iterations: NonZeroU32,
        },
    }

    #[derive(Debug, Clone, Copy)]
    pub(crate) enum Pbkdf2Algorithm {
        Sha256,
    }

    impl Pbkdf2Algorithm {
        pub(crate) fn function(self) -> fn(&[u8], &[u8], u32, &mut [u8]) {
            match self {
                Self::Sha256 => pbkdf2::<Hmac<Sha256>>,
            }
        }
    }

    impl<'de> Deserialize<'de> for Prelogin {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Visitor;
            impl<'de> de::Visitor<'de> for Visitor {
                type Value = Prelogin;
                fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                    f.write_str("prelogin data")
                }
                fn visit_map<A: de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> Result<Self::Value, A::Error> {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "snake_case")]
                    enum FirstKey {
                        Kdf,
                    }

                    map.next_key::<FirstKey>()?
                        .ok_or_else(|| de::Error::missing_field("Kdf"))?;

                    #[derive(Deserialize)]
                    struct Pbkdf2Opts {
                        #[serde(rename = "kdfIterations")]
                        iterations: NonZeroU32,
                    }

                    Ok(match map.next_value::<u32>()? {
                        0 => {
                            let opts = Pbkdf2Opts::deserialize(MapAccessDeserializer::new(map))?;
                            Prelogin::Pbkdf2 {
                                algorithm: Pbkdf2Algorithm::Sha256,
                                iterations: opts.iterations,
                            }
                        }
                        n => return Err(de::Error::custom(format_args!("unknown KDF number {n}"))),
                    })
                }
            }
            deserializer.deserialize_map(Visitor)
        }
    }

    use hmac::Hmac;
    use pbkdf2::pbkdf2;
    use serde::de;
    use serde::de::value::MapAccessDeserializer;
    use serde::Deserialize;
    use serde::Deserializer;
    use serde::Serialize;
    use sha2::Sha256;
    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use std::io;
    use std::num::NonZeroU32;
}

pub(crate) fn master_key(prelogin: &Prelogin, email: &str, master_password: &str) -> MasterKey {
    let mut master_key = MasterKey::zeroed();

    match prelogin {
        Prelogin::Pbkdf2 {
            algorithm,
            iterations,
        } => {
            algorithm.function()(
                master_password.as_bytes(),
                to_lowercase_cow(email).as_bytes(),
                iterations.get(),
                &mut *master_key.0,
            );
        }
    }

    master_key
}

pub(crate) use login::login;
pub(crate) mod login {
    pub(crate) fn login(
        http: &ureq::Agent,
        client_id: &str,
        device: auth::Device<'_>,
        scopes: auth::Scopes,
        email: &str,
        master_password: &str,
    ) -> Result<(Prelogin, MasterKey, auth::Token), Error> {
        inner(http, client_id, device, scopes, email, master_password).map_err(|kind| Error {
            kind,
            email: email.into(),
        })
    }

    fn inner(
        http: &ureq::Agent,
        client_id: &str,
        device: auth::Device<'_>,
        scopes: auth::Scopes,
        email: &str,
        master_password: &str,
    ) -> Result<(Prelogin, MasterKey, auth::Token), ErrorKind> {
        let prelogin = prelogin(http, email).map_err(ErrorKind::Prelogin)?;
        let master_key = master_key(&prelogin, email, master_password);

        const MASTER_PASSWORD_HASH_LEN: usize = 32;
        let mut master_password_hash = Zeroizing::new([0; MASTER_PASSWORD_HASH_LEN]);

        match prelogin {
            Prelogin::Pbkdf2 { algorithm, .. } => {
                algorithm.function()(
                    &*master_key.0,
                    master_password.as_bytes(),
                    1,
                    &mut *master_password_hash,
                );
            }
        }

        let mut password =
            Zeroizing::new(String::with_capacity(MASTER_PASSWORD_HASH_LEN * 4 / 3 + 4));
        base64::encode_config_buf(master_password_hash, base64::STANDARD, &mut password);

        let mut device_type_buf = itoa::Buffer::new();
        let device_type = device_type_buf.format(device.r#type as u8);

        let response = http
            .post("https://identity.bitwarden.com/connect/token")
            .set(
                "Auth-Email",
                &base64::encode_config(email, base64::URL_SAFE),
            )
            .set("Accept", "application/json")
            .set("Device-Type", device_type)
            .set("Cache-Control", "no-store")
            .set("User-Agent", "rust")
            .send_form(&[
                ("grant_type", "password"),
                ("username", email),
                ("password", &*password),
                ("scope", &*scopes.to_string()),
                ("client_id", client_id),
                ("deviceName", device.name),
                (
                    "deviceIdentifier",
                    device
                        .identifier
                        .as_hyphenated()
                        .encode_lower(&mut [0; uuid::fmt::Hyphenated::LENGTH]),
                ),
                ("deviceType", device_type),
            ])?
            .into_json::<AccessTokenResponse>()
            .map_err(ErrorKind::Body)?;

        Ok((prelogin, master_key, response.into_token()))
    }

    #[derive(Debug)]
    pub(crate) struct Error {
        pub(crate) kind: ErrorKind,
        email: Box<str>,
    }

    #[derive(Debug)]
    pub(crate) enum ErrorKind {
        Prelogin(prelogin::Error),
        InvalidCredentials(InvalidCredentials),
        Status(Status),
        Transport(Box<ureq::Transport>),
        Body(io::Error),
    }

    impl Display for Error {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(f, "failed to log in to account {}", self.email)
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match &self.kind {
                ErrorKind::Prelogin(e) => Some(e),
                ErrorKind::InvalidCredentials(e) => Some(e),
                ErrorKind::Status(e) => Some(e),
                ErrorKind::Transport(e) => Some(e),
                ErrorKind::Body(e) => Some(e),
            }
        }
    }

    impl From<ureq::Error> for ErrorKind {
        fn from(error: ureq::Error) -> Self {
            match error {
                ureq::Error::Status(status, res) => {
                    let body = match res.into_string() {
                        Ok(body) => {
                            #[derive(Deserialize)]
                            struct ErrorResponse {
                                #[serde(rename = "ErrorModel")]
                                error_model: ErrorModel,
                                error_description: String,
                            }

                            #[derive(Deserialize)]
                            #[serde(rename_all = "PascalCase")]
                            struct ErrorModel {
                                message: String,
                            }

                            match serde_json::from_str::<ErrorResponse>(&body) {
                                Ok(response) => {
                                    if response.error_description == "invalid_username_or_password"
                                    {
                                        return ErrorKind::InvalidCredentials(InvalidCredentials);
                                    }
                                    Body::Message(response.error_model.message)
                                }
                                Err(_) => Body::Other(body),
                            }
                        }
                        Err(e) => Body::Error(e),
                    };
                    ErrorKind::Status(Status { code: status, body })
                }
                ureq::Error::Transport(e) => ErrorKind::Transport(Box::new(e)),
            }
        }
    }

    #[derive(Debug)]
    pub(crate) struct InvalidCredentials;

    impl Display for InvalidCredentials {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("invalid username or password; try again")
        }
    }

    impl std::error::Error for InvalidCredentials {}

    #[derive(Debug)]
    pub(crate) struct Status {
        code: u16,
        body: Body,
    }

    #[derive(Debug)]
    enum Body {
        Message(String),
        Other(String),
        Error(io::Error),
    }

    impl Display for Status {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            match &self.body {
                Body::Message(e) => f.write_str(e),
                Body::Other(s) => write!(f, "status {}; body {s:?}", self.code),
                Body::Error(_) => write!(f, "status {} and error reading body", self.code),
            }
        }
    }

    impl std::error::Error for Status {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match &self.body {
                Body::Error(e) => Some(e),
                _ => None,
            }
        }
    }

    use super::AccessTokenResponse;
    use crate::auth;
    use crate::auth::master_key;
    use crate::auth::prelogin;
    use crate::auth::Prelogin;
    use rofi_bw_common::MasterKey;
    use serde::Deserialize;
    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use std::io;
    use zeroize::Zeroizing;
}

pub(crate) use refresh::refresh;
pub(crate) mod refresh {
    pub(crate) fn refresh(
        http: &ureq::Agent,
        client_id: &str,
        refresh_token: &str,
    ) -> Result<auth::Token, Error> {
        let response = http
            .post("https://identity.bitwarden.com/connect/token")
            .send_form(&[
                ("grant_type", "refresh_token"),
                ("client_id", client_id),
                ("refresh_token", refresh_token),
            ])
            .map_err(|e| {
                if let ureq::Error::Status(400, _) = e {
                    Error::SessionExpired(SessionExpired)
                } else {
                    Error::Http(Box::new(e))
                }
            })?
            .into_json::<AccessTokenResponse>()
            .map_err(Error::Body)?;

        Ok(response.into_token())
    }

    #[derive(Debug)]
    pub(crate) enum Error {
        SessionExpired(SessionExpired),
        Http(Box<ureq::Error>),
        Body(io::Error),
    }

    impl Display for Error {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("failed to refresh token")
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn 'static + std::error::Error)> {
            match self {
                Self::SessionExpired(e) => Some(e),
                Self::Http(e) => Some(e),
                Self::Body(e) => Some(e),
            }
        }
    }

    #[derive(Debug)]
    pub(crate) struct SessionExpired;

    impl Display for SessionExpired {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("session expired")
        }
    }

    impl std::error::Error for SessionExpired {}

    use super::AccessTokenResponse;
    use crate::auth;
    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use std::io;
}

#[derive(Debug)]
pub(crate) struct Token {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) expires: SystemTime,
}

impl Token {
    pub(crate) fn set_expired(&mut self) {
        self.access_token.clear();
    }

    pub(crate) fn is_expired(&self) -> bool {
        self.access_token.is_empty() || SystemTime::now() >= self.expires
    }
}

// from:
// https://github.com/bitwarden/server/blob/master/src/Core/Enums/DeviceType.cs
#[derive(Debug, Clone, Copy)]
pub(crate) enum DeviceType {
    Android = 0,
    Ios = 1,
    ChromeExtension = 2,
    FirefoxExtension = 3,
    OperaExtension = 4,
    EdgeExtension = 5,
    WindowsDesktop = 6,
    MacOsDesktop = 7,
    LinuxDesktop = 8,
    ChromeBrowser = 9,
    FirefoxBrowser = 10,
    OperaBrowser = 11,
    EdgeBrowser = 12,
    IeBrowser = 13,
    UnknownBrowser = 14,
    AndroidAmazon = 15,
    Uwp = 16,
    SafariBrowser = 17,
    VivaldiBrowser = 18,
    VivaldiExtension = 19,
    SafariExtension = 20,
}

impl DeviceType {
    pub(crate) const DISPLAY_NAMES: [(Self, &'static str); 21] = [
        (Self::Android, "Android"),
        (Self::Ios, "iOS"),
        (Self::ChromeExtension, "Chrome Extension"),
        (Self::FirefoxExtension, "Firefox Extension"),
        (Self::OperaExtension, "Opera Extension"),
        (Self::EdgeExtension, "Edge Extension"),
        (Self::WindowsDesktop, "Windows"),
        (Self::MacOsDesktop, "macOS"),
        (Self::LinuxDesktop, "Linux"),
        (Self::ChromeBrowser, "Chrome"),
        (Self::FirefoxBrowser, "Firefox"),
        (Self::OperaBrowser, "Opera"),
        (Self::EdgeBrowser, "Edge"),
        (Self::IeBrowser, "Internet Explorer"),
        (Self::UnknownBrowser, "Unknown Browser"),
        (Self::AndroidAmazon, "Android"),
        (Self::Uwp, "UWP"),
        (Self::SafariBrowser, "Safari"),
        (Self::VivaldiBrowser, "Vivaldi"),
        (Self::VivaldiExtension, "Vivaldi Extension"),
        (Self::SafariExtension, "Safari Extension"),
    ];
}

bitflags! {
    pub(crate) struct Scopes: u8 {
        const API = 1;
        const OFFLINE_ACCESS = 2;
    }
}

impl Display for Scopes {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        const STRINGS: &[(Scopes, &str)] = &[
            (Scopes::API, "api"),
            (Scopes::OFFLINE_ACCESS, "offline_access"),
        ];
        let mut needs_space = false;
        for &(scope, string) in STRINGS {
            if self.contains(scope) {
                if needs_space {
                    f.write_str(" ")?;
                }
                f.write_str(string)?;
                needs_space = true;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Device<'name> {
    pub(crate) name: &'name str,
    pub(crate) identifier: Uuid,
    pub(crate) r#type: DeviceType,
}

#[derive(Deserialize)]
struct AccessTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}
impl AccessTokenResponse {
    fn into_token(self) -> Token {
        Token {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            expires: SystemTime::now() + Duration::from_secs(self.expires_in),
        }
    }
}

fn to_lowercase_cow(s: &str) -> Cow<'_, str> {
    if s.chars().all(char::is_lowercase) {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(s.to_lowercase())
    }
}

use bitflags::bitflags;
use rofi_bw_common::MasterKey;
use serde::Deserialize;
use std::borrow::Cow;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::str;
use std::time::Duration;
use std::time::SystemTime;
use uuid::Uuid;
