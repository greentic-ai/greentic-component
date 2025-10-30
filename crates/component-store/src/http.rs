#[cfg(feature = "http")]
use reqwest::blocking::Client;
#[cfg(feature = "http")]
use reqwest::header::{ACCEPT, USER_AGENT};
#[cfg(feature = "http")]
use url::Url;

use crate::StoreError;

#[cfg(feature = "http")]
pub fn build_client() -> Result<Client, StoreError> {
    Client::builder()
        .user_agent("greentic-component/0.1")
        .build()
        .map_err(StoreError::from)
}

#[cfg(feature = "http")]
pub fn fetch(client: &Client, url: &Url) -> Result<Vec<u8>, StoreError> {
    let response = client
        .get(url.clone())
        .header(USER_AGENT, "greentic-component/0.1")
        .header(ACCEPT, "application/wasm,application/octet-stream")
        .send()?;
    let response = response.error_for_status()?;
    Ok(response.bytes()?.to_vec())
}

#[cfg(not(feature = "http"))]
pub fn build_client() -> Result<(), StoreError> {
    Err(StoreError::UnsupportedScheme("http".into()))
}

#[cfg(not(feature = "http"))]
pub fn fetch(_client: &(), _url: &url::Url) -> Result<Vec<u8>, StoreError> {
    Err(StoreError::UnsupportedScheme("http".into()))
}
