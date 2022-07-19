pub(crate) fn prelogin(http: &ureq::Agent, email: &str) -> anyhow::Result<Prelogin> {
    #[derive(Serialize)]
    struct Body<'email> {
        email: &'email str,
    }

    http.post("https://vault.bitwarden.com/api/accounts/prelogin")
        .send_json(Body { email })
        .context("prelogin failed")?
        .into_json()
        .context("prelogin body reading failed")
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
    fn function(self) -> fn(&[u8], &[u8], u32, &mut [u8]) {
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
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
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

pub(crate) fn login(
    http: &ureq::Agent,
    client_id: &str,
    device: Device<'_, '_>,
    scopes: Scopes,
    email: &str,
    master_password: &str,
) -> anyhow::Result<(Prelogin, MasterKey, AccessToken)> {
    let prelogin = prelogin(http, email)?;
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

    let mut password = Zeroizing::new(String::with_capacity(MASTER_PASSWORD_HASH_LEN * 4 / 3 + 4));
    base64::encode_config_buf(master_password_hash, base64::STANDARD, &mut password);

    let mut device_type_buf = itoa::Buffer::new();
    let device_type = device_type_buf.format(device.r#type as u8);

    let response =
        http.post("https://identity.bitwarden.com/connect/token")
            .set(
                "Auth-Email",
                &*base64::encode_config(&email, base64::URL_SAFE),
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
                ("deviceIdentifier", device.identifier),
                ("deviceType", device_type),
            ])
            .map_err(|e| match e {
                ureq::Error::Status(status, res) => match res.into_string() {
                    Ok(body) => {
                        #[derive(Deserialize)]
                        struct ErrorResponse {
                            #[serde(rename = "ErrorModel")]
                            error_model: ErrorModel,
                        }

                        #[derive(Deserialize)]
                        #[serde(rename_all = "PascalCase")]
                        struct ErrorModel {
                            message: String,
                        }

                        if let Ok(response) = serde_json::from_str::<ErrorResponse>(&body) {
                            anyhow!("{}", response.error_model.message)
                        } else {
                            anyhow!("status {status}: body {body:?}")
                        }
                    }
                    Err(e) => anyhow::Error::new(e)
                        .context(format!("status {status} and error reading body")),
                },
                e @ ureq::Error::Transport(_) => anyhow::Error::new(e),
            })
            .context("failed to send token request")?
            .into_json::<AccessTokenResponse>()
            .context("failed to read token response body")?;

    Ok((prelogin, master_key, response.into_token()))
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum DeviceType {
    // Android = 0,
    // Ios = 1,
    // ChromeExtension = 2,
    // FirefoxExtension = 3,
    // OperaExtension = 4,
    // EdgeExtension = 5,
    // WindowsDesktop = 6,
    // MacOsDesktop = 7,
    LinuxDesktop = 8,
    // ChromeBrowser = 9,
    // FirefoxBrowser = 10,
    // OperaBrowser = 11,
    // EdgeBrowser = 12,
    // IEBrowser = 13,
    // UnknownBrowser = 14,
    // AndroidAmazon = 15,
    // Uwp = 16,
    // SafariBrowser = 17,
    // VivaldiBrowser = 18,
    // VivaldiExtension = 19,
    // SafariExtension = 20,
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
pub(crate) struct Device<'name, 'identifier> {
    pub(crate) name: &'name str,
    pub(crate) identifier: &'identifier str,
    pub(crate) r#type: DeviceType,
}

pub(crate) fn refresh_token(
    http: &ureq::Agent,
    client_id: &str,
    refresh_token: &str,
) -> Result<AccessToken, RefreshError> {
    let response = http
        .post("https://identity.bitwarden.com/connect/token")
        .send_form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token),
        ])
        .map_err(|e| {
            if let ureq::Error::Status(400, _) = e {
                RefreshError::SessionExpired(SessionExpired)
            } else {
                RefreshError::Http(e)
            }
        })?
        .into_json::<AccessTokenResponse>()
        .map_err(RefreshError::Body)?;

    Ok(response.into_token())
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum RefreshError {
    SessionExpired(SessionExpired),
    Http(ureq::Error),
    Body(io::Error),
}

impl Display for RefreshError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("failed to refresh token")
    }
}

impl std::error::Error for RefreshError {
    fn source(&self) -> Option<&(dyn de::StdError + 'static)> {
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

#[derive(Debug)]
pub(crate) struct AccessToken {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) expires: SystemTime,
}

impl AccessToken {
    pub(crate) fn is_expired(&self) -> bool {
        SystemTime::now() >= self.expires
    }
}

#[derive(Deserialize)]
struct AccessTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}
impl AccessTokenResponse {
    fn into_token(self) -> AccessToken {
        AccessToken {
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

use anyhow::anyhow;
use anyhow::Context as _;
use bitflags::bitflags;
use hmac::Hmac;
use pbkdf2::pbkdf2;
use rofi_bw_common::MasterKey;
use serde::de;
use serde::de::value::MapAccessDeserializer;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use sha2::Sha256;
use std::borrow::Cow;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io;
use std::num::NonZeroU32;
use std::str;
use std::time::Duration;
use std::time::SystemTime;
use zeroize::Zeroizing;
