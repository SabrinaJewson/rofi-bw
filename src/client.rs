pub(crate) trait TokenSource {
    type Error;

    fn access_token(&mut self) -> Result<&str, Self::Error>;
}

pub(crate) struct Client<Ts: TokenSource, B: Deref<Target = str>> {
    pub(crate) http: ureq::Agent,
    pub(crate) token_source: Ts,
    pub(crate) base_url: B,
}

impl<Ts: TokenSource, B: Deref<Target = str>> Client<Ts, B> {
    pub(crate) fn sync(&mut self) -> Result<String, SyncError<Ts::Error>> {
        let token = self.token_source.access_token().map_err(SyncError::Token)?;

        let data = self
            .http
            .get(&*format!("{}/sync?excludeDomains=true", &*self.base_url))
            .set("Authorization", &*format!("Bearer {token}"))
            .call()
            .map_err(SyncError::Http)?
            .into_string()
            .map_err(SyncError::Body)?;

        Ok(data)
    }
}

#[derive(Debug)]
pub(crate) enum SyncError<T> {
    Token(T),
    Http(ureq::Error),
    Body(io::Error),
}

impl<T> Display for SyncError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("failed to synchronize with Bitwarden server")
    }
}

impl<T: 'static + Error> Error for SyncError<T> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Token(e) => Some(e),
            Self::Http(e) => Some(e),
            Self::Body(e) => Some(e),
        }
    }
}

use std::error::Error;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io;
use std::ops::Deref;
