//! HTTPS requests: the release list and file downloads.
//!
//! TLS is rustls with the *ring* provider; the trusted root certificates are read from the operating system
//! (Windows certificate store, macOS keychain, the usual certificate files on Linux), so no certificate bundle
//! ships with Kanavos. Only HTTPS is allowed, including on redirects. Proxies are taken from the usual
//! environment variables.
//!
//! Each request sends only what HTTP needs plus `User-Agent: Kanavos/<version> (<os>; <arch>)`.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ureq::Agent;
use ureq::http::{Response, StatusCode};
use ureq::tls::{Certificate, RootCerts, TlsConfig, TlsProvider};

use crate::UpdateError;

/// Largest release list accepted, in bytes.
const MAX_LIST_BYTES: u64 = 8 << 20;
/// Largest checksum or signature file accepted, in bytes.
const MAX_SMALL_BYTES: u64 = 64 << 10;
/// Largest package accepted, in bytes.
pub const MAX_ARCHIVE_BYTES: u64 = 512 << 20;

fn tls_config() -> Result<TlsConfig, UpdateError> {
    let found = rustls_native_certs::load_native_certs();
    let roots: Vec<Certificate<'static>> =
        found.certs.iter().map(|c| Certificate::from_der(c.as_ref()).to_owned()).collect();
    if roots.is_empty() {
        let why = found.errors.first().map(ToString::to_string).unwrap_or_default();
        return Err(UpdateError::Network(format!("no trusted root certificates found on this system {why}")));
    }
    Ok(TlsConfig::builder()
        .provider(TlsProvider::Rustls)
        .root_certs(RootCerts::new_with_certs(&roots))
        .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .build())
}

/// An HTTPS-only agent. `body_timeout` bounds the time to receive a whole response body.
pub(crate) fn agent(user_agent: &str, body_timeout: Duration) -> Result<Agent, UpdateError> {
    let config = Agent::config_builder()
        .tls_config(tls_config()?)
        .https_only(true)
        .http_status_as_error(false)
        .user_agent(user_agent)
        .max_redirects(10)
        .timeout_resolve(Some(Duration::from_secs(20)))
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_send_request(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_recv_body(Some(body_timeout))
        .build();
    Ok(Agent::new_with_config(config))
}

fn network(e: ureq::Error) -> UpdateError {
    match e {
        ureq::Error::BodyExceedsLimit(limit) => UpdateError::TooLarge { name: String::new(), limit },
        other => UpdateError::Network(other.to_string()),
    }
}

fn header<'a, B>(resp: &'a Response<B>, name: &str) -> Option<&'a str> {
    resp.headers().get(name).and_then(|v| v.to_str().ok())
}

/// Minutes until the rate limit resets, from `Retry-After` (seconds) or `X-RateLimit-Reset` (epoch seconds).
fn retry_minutes<B>(resp: &Response<B>) -> Option<u64> {
    if let Some(secs) = header(resp, "retry-after").and_then(|v| v.trim().parse::<u64>().ok()) {
        return Some(secs.div_ceil(60).max(1));
    }
    let reset = header(resp, "x-ratelimit-reset").and_then(|v| v.trim().parse::<u64>().ok())?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(reset.saturating_sub(now).div_ceil(60).max(1))
}

/// Turns a non-success status into an error.
fn status_error<B>(resp: &Response<B>, url: &str) -> UpdateError {
    let status = resp.status();
    if status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::FORBIDDEN {
        UpdateError::RateLimited { minutes: retry_minutes(resp) }
    } else {
        UpdateError::Http { status: status.as_u16(), url: url.to_owned() }
    }
}

/// Fetches the release list (JSON text).
pub(crate) fn get_release_list(agent: &Agent, url: &str) -> Result<String, UpdateError> {
    let mut resp = agent
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(network)?;
    if !resp.status().is_success() {
        return Err(status_error(&resp, url));
    }
    resp.body_mut().with_config().limit(MAX_LIST_BYTES).read_to_string().map_err(network)
}

/// Fetches a small text file; `None` when the server says it does not exist.
pub(crate) fn get_small_text(agent: &Agent, url: &str) -> Result<Option<String>, UpdateError> {
    let mut resp = agent.get(url).header("Accept", "application/octet-stream").call().map_err(network)?;
    if resp.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(status_error(&resp, url));
    }
    resp.body_mut().with_config().limit(MAX_SMALL_BYTES).read_to_string().map(Some).map_err(network)
}

/// Downloads `url` into the new file `path`, calling `progress(done, total)` as bytes arrive and stopping when
/// `cancel` is set. `expected` is the announced size (0 when unknown).
pub(crate) fn download_file(
    agent: &Agent,
    url: &str,
    name: &str,
    expected: u64,
    path: &Path,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<(), UpdateError> {
    let too_large = || UpdateError::TooLarge { name: name.to_owned(), limit: MAX_ARCHIVE_BYTES };
    if expected > MAX_ARCHIVE_BYTES {
        return Err(too_large());
    }
    let resp = agent.get(url).header("Accept", "application/octet-stream").call().map_err(network)?;
    if !resp.status().is_success() {
        return Err(status_error(&resp, url));
    }
    let length = header(&resp, "content-length").and_then(|v| v.trim().parse::<u64>().ok());
    if length.is_some_and(|l| l > MAX_ARCHIVE_BYTES || (expected > 0 && l != expected)) {
        return Err(UpdateError::BadPackage(format!("{name}: the server sent a different size")));
    }
    let total = if expected > 0 { expected } else { length.unwrap_or(0) };
    let limit = if total > 0 { total } else { MAX_ARCHIVE_BYTES };
    let mut reader = resp.into_body().into_with_config().limit(limit.saturating_add(1)).reader();
    let mut file = File::create_new(path).map_err(|e| UpdateError::io(path, e))?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    progress(0, total);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(UpdateError::Cancelled);
        }
        let n = reader.read(&mut buf).map_err(|e| UpdateError::Network(e.to_string()))?;
        if n == 0 {
            break;
        }
        let chunk = buf.get(..n).ok_or_else(too_large)?;
        done = done.saturating_add(n as u64);
        if done > limit {
            return Err(too_large());
        }
        file.write_all(chunk).map_err(|e| UpdateError::io(path, e))?;
        progress(done, total);
    }
    if total > 0 && done != total {
        return Err(UpdateError::Network(format!(
            "{name}: the download ended early ({done} of {total} bytes)"
        )));
    }
    file.sync_all().map_err(|e| UpdateError::io(path, e))
}
