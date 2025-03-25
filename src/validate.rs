use crate::errors::ApiError;
use anyhow::{bail, Result};
use axum::{
    async_trait,
    extract::{FromRequest, Request},
    Json,
};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::de::DeserializeOwned;

pub struct ValidatedJson<T>(pub T);

#[async_trait]
impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(payload) = Json::<T>::from_request(req, state).await?;
        Ok(ValidatedJson(payload))
    }
}

// Restricts to letters, digits, underscores, hyphens, dots. Must be 1–256 characters long.
static NAME_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-zA-Z0-9._-]{1,256}$").expect("Failed to compile NAME_REGEX"));

pub fn validate_name(name: &str) -> Result<()> {
    if !NAME_REGEX.is_match(name) {
        bail!(
            "Invalid name: must be 1–256 characters long and only contain letters, \
            digits, underscores, hyphens, or periods."
        );
    }
    Ok(())
}

pub fn validate_namespace(namespace: &str) -> Result<()> {
    if !NAME_REGEX.is_match(namespace) {
        bail!(
            "Invalid namespace: must be 1–256 characters long and only contain letters, \
            digits, underscores, hyphens, or periods."
        );
    }
    Ok(())
}
