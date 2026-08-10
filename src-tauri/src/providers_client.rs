//! HTTP-client construction for normal model generation and short-lived Settings probes.

use std::time::Duration;

use reqwest::{
    blocking::Client,
    header::{HeaderMap, HeaderValue, ACCEPT_ENCODING},
};

use crate::{
    error::{AppError, AppResult},
    media::MediaStore,
    repository::Repository,
};

use super::ProviderClient;

const LANGUAGE_REQUEST_TIMEOUT: Duration = Duration::from_secs(180);
pub(crate) const MODEL_PROBE_TIMEOUT: Duration = Duration::from_secs(20);

impl ProviderClient {
    /// Create the provider boundary with a finite timeout suitable for background-generation tasks.
    pub fn new(repository: Repository, media: MediaStore) -> AppResult<Self> {
        Self::with_timeout(repository, media, LANGUAGE_REQUEST_TIMEOUT)
    }

    /// Create the provider boundary used by Settings probes, which must fail promptly so the desktop stays responsive.
    pub(crate) fn for_model_probe(repository: Repository, media: MediaStore) -> AppResult<Self> {
        Self::with_timeout(repository, media, MODEL_PROBE_TIMEOUT)
    }

    fn with_timeout(
        repository: Repository,
        media: MediaStore,
        timeout: Duration,
    ) -> AppResult<Self> {
        let mut direct_model_headers = HeaderMap::new();
        // Model providers are called directly; avoid compressed responses that an intermediary may alter.
        direct_model_headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
        Ok(Self {
            repository,
            media,
            client: Client::builder()
                .default_headers(direct_model_headers)
                .connect_timeout(timeout)
                .timeout(timeout)
                .build()
                .map_err(|error| AppError::External(error.to_string()))?,
        })
    }
}
