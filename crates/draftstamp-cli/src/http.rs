//! Native HTTP for opentimestamps-lite: blocking `ureq`, a timeout, and no identifying headers.

use std::time::Duration;

use draftstamp_core::{Http, OtsError, Response};
use ureq::Agent;

const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_BODY: u64 = 1 << 20;

/// The real network.
pub struct Native(Agent);

impl Native {
    /// An agent with a 10-second timeout that returns every HTTP status as a response.
    pub fn new() -> Native {
        let config = Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .user_agent(concat!("draftstamp/", env!("CARGO_PKG_VERSION")))
            .build();
        Native(config.into())
    }
}

impl Default for Native {
    fn default() -> Self {
        Native::new()
    }
}

fn read(
    result: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<Response, OtsError> {
    let mut response = result.map_err(|e| OtsError::Http(e.to_string()))?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_BODY)
        .read_to_vec()
        .map_err(|e| OtsError::Http(e.to_string()))?;
    Ok(Response { status, body })
}

impl Http for Native {
    fn get(&self, url: &str) -> Result<Response, OtsError> {
        read(self.0.get(url).call())
    }

    fn post(&self, url: &str, body: &[u8]) -> Result<Response, OtsError> {
        read(self.0.post(url).send(body))
    }
}
