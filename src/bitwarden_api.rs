#[derive(Debug, Clone, Copy)]
pub(crate) struct Client<'http, 'access_token, 'base_url> {
    http: &'http ureq::Agent,
    access_token: &'access_token str,
    base_url: &'base_url str,
}

impl<'http, 'access_token, 'base_url> Client<'http, 'access_token, 'base_url> {
    pub(crate) fn new(http: &'http ureq::Agent, access_token: &'access_token str) -> Self {
        Self {
            http,
            access_token,
            base_url: "https://vault.bitwarden.com/api",
        }
    }

    pub(crate) fn sync(self) -> Result<String, SyncError> {
        let data = self
            .http
            .get(&format!("{}/sync?excludeDomains=true", &self.base_url))
            .set("Authorization", &format!("Bearer {}", self.access_token))
            .set("Accept", "application/json")
            .call()?
            .into_string()?;

        Ok(data)
    }
}

#[derive(Debug)]
pub(crate) enum SyncError {
    Http(Box<ureq::Error>),
    Body(io::Error),
}

impl From<ureq::Error> for SyncError {
    fn from(error: ureq::Error) -> Self {
        Self::Http(Box::new(error))
    }
}

impl From<io::Error> for SyncError {
    fn from(error: io::Error) -> Self {
        Self::Body(error)
    }
}

impl Display for SyncError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("failed to synchronize with Bitwarden server")
    }
}

impl std::error::Error for SyncError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(e) => Some(e),
            Self::Body(e) => Some(e),
        }
    }
}

use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io;
