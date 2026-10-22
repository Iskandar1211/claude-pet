use std::time::Duration;

use anyhow::{Context, Result};

pub fn build_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .context("build http client")
}
