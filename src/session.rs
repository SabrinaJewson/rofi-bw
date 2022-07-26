pub(crate) struct Session<'http, 'client_id> {
    http: &'http ureq::Agent,
    client_id: &'client_id str,
    master_key: MasterKey,
    token: auth::Token,
    account_data: String,
}

impl<'http, 'client_id> Session<'http, 'client_id> {
    pub(crate) fn new(
        http: &'http ureq::Agent,
        cache_dir: &fs::Path,
        client_id: &'client_id str,
        device: auth::Device<'_>,
        email: &str,
        master_password: &str,
    ) -> anyhow::Result<Self> {
        let cache_key = cache::Key::new(email, master_password)?;
        let cache = cache::load(cache_dir, &cache_key);

        let validated_cache = match cache {
            Some(cache) => match auth::refresh_token(http, client_id, &*cache.refresh_token) {
                Ok(token) => Some((cache.prelogin, token)),
                Err(auth::RefreshError::SessionExpired(_)) => None,
                Err(e) => return Err(e.into()),
            },
            None => None,
        };

        let (master_key, token) = match validated_cache {
            Some((prelogin, token)) => {
                let master_key = auth::master_key(&prelogin, email, master_password);
                (master_key, token)
            }
            None => {
                let (prelogin, master_key, token) = auth::login(
                    http,
                    client_id,
                    device,
                    auth::Scopes::all(),
                    email,
                    master_password,
                )?;
                cache::store(
                    cache_dir,
                    &cache_key,
                    CacheRef {
                        refresh_token: &*token.refresh_token,
                        prelogin: &prelogin,
                    },
                );
                (master_key, token)
            }
        };

        let account_data = bitwarden_api::Client::new(http, &*token.access_token).sync()?;

        Ok(Self {
            http,
            client_id,
            master_key,
            token,
            account_data,
        })
    }

    fn client(&mut self) -> Result<bitwarden_api::Client<'http, '_, 'static>, auth::RefreshError> {
        if self.token.is_expired() {
            self.token =
                auth::refresh_token(self.http, self.client_id, &*self.token.refresh_token)?;
        }
        Ok(bitwarden_api::Client::new(
            self.http,
            &*self.token.access_token,
        ))
    }

    pub(crate) fn resync(&mut self) -> Result<(), ResyncError> {
        // Force a token refresh. This is needed to make sure that our session hasn't expired; if it
        // has, it’s likely the master password or KDF iterations have changed, and so we need to
        // reinstate the session.
        self.token.set_expired();

        self.account_data = self.client()?.sync()?;

        Ok(())
    }

    pub(crate) fn master_key(&self) -> &MasterKey {
        &self.master_key
    }

    pub(crate) fn account_data(&self) -> &str {
        &*self.account_data
    }
}

#[derive(Debug)]
pub(crate) enum ResyncError {
    RefreshToken(auth::RefreshError),
    Sync(bitwarden_api::SyncError),
}

impl From<auth::RefreshError> for ResyncError {
    fn from(error: auth::RefreshError) -> Self {
        Self::RefreshToken(error)
    }
}

impl From<bitwarden_api::SyncError> for ResyncError {
    fn from(error: bitwarden_api::SyncError) -> Self {
        Self::Sync(error)
    }
}

impl Display for ResyncError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("failed to resync")
    }
}

impl std::error::Error for ResyncError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::RefreshToken(e) => Some(e),
            Self::Sync(e) => Some(e),
        }
    }
}

use crate::auth;
use crate::bitwarden_api;
use crate::cache;
use crate::cache::CacheRef;
use rofi_bw_common::fs;
use rofi_bw_common::MasterKey;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;