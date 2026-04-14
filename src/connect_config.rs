use anyhow::{Result, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use url::Url;

pub const MAX_CONNECT_URL_BYTES: usize = 250;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltConnectUrl {
    pub url: String,
    pub included_name: Option<String>,
    pub byte_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectPayload {
    pub v: u32,
    #[serde(rename = "type")]
    pub payload_type: String,
    pub server_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub credential: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
}

pub fn build_connect_url(
    server_url: &str,
    name: Option<String>,
    credential: String,
    token_id: Option<String>,
) -> Result<String> {
    validate_server_url(server_url)?;
    let payload = ConnectPayload {
        v: 1,
        payload_type: "connect".to_string(),
        server_url: server_url.to_string(),
        name,
        credential,
        token_id,
    };
    let json = serde_json::to_vec(&payload)?;
    let encoded = URL_SAFE_NO_PAD.encode(json);
    let url = format!("cmdock://connect?payload={encoded}");
    if url.len() > MAX_CONNECT_URL_BYTES {
        bail!(
            "connect-config URL is {} bytes, which exceeds the {} byte budget",
            url.len(),
            MAX_CONNECT_URL_BYTES
        );
    }
    Ok(url)
}

pub fn build_connect_url_with_fallback(
    server_url: &str,
    preferred_name: Option<String>,
    credential: String,
    token_id: Option<String>,
) -> Result<BuiltConnectUrl> {
    let normalized_name = preferred_name.and_then(normalize_name);
    if let Some(name) = normalized_name.clone() {
        match build_connect_url(
            server_url,
            Some(name.clone()),
            credential.clone(),
            token_id.clone(),
        ) {
            Ok(url) => {
                return Ok(BuiltConnectUrl {
                    byte_len: url.len(),
                    url,
                    included_name: Some(name),
                });
            }
            Err(err) if err.to_string().contains("exceeds the 250 byte budget") => {}
            Err(err) => return Err(err),
        }
    }

    let url = build_connect_url(server_url, None, credential, token_id)?;
    Ok(BuiltConnectUrl {
        byte_len: url.len(),
        url,
        included_name: None,
    })
}

fn validate_server_url(server_url: &str) -> Result<()> {
    let parsed = Url::parse(server_url)?;
    if parsed.scheme() != "https" {
        bail!("connect QR codes require an HTTPS server URL");
    }
    if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
        bail!("server URL must be scheme + authority only");
    }
    Ok(())
}

fn normalize_name(name: String) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_url_round_trip_shape() {
        let url = build_connect_url(
            "https://tasks.example.com",
            Some("My Server".into()),
            "opaque".into(),
            Some("cc_1234".into()),
        )
        .unwrap();
        assert!(url.starts_with("cmdock://connect?payload="));
        assert!(url.contains("payload="));
    }

    #[test]
    fn connect_url_with_fallback_keeps_name_when_it_fits() {
        let built = build_connect_url_with_fallback(
            "https://tasks.example.com",
            Some("My Server".into()),
            "opaque".into(),
            Some("cc_1234".into()),
        )
        .unwrap();
        assert_eq!(built.included_name.as_deref(), Some("My Server"));
        assert!(built.byte_len <= MAX_CONNECT_URL_BYTES);
    }

    #[test]
    fn connect_url_with_fallback_drops_name_to_fit_budget() {
        let long_name = "Display Name ".repeat(8);
        let built = build_connect_url_with_fallback(
            "https://cmdock-tasks-staging.example.com",
            Some(long_name),
            "FYnel6MP4Sd6XO1jPp9FE0YM".into(),
            Some("cc_0123456789abcd".into()),
        )
        .unwrap();
        assert_eq!(built.included_name, None);
        assert!(built.byte_len <= MAX_CONNECT_URL_BYTES);
    }

    #[test]
    fn connect_url_with_fallback_still_fails_if_payload_cannot_fit() {
        let err = build_connect_url_with_fallback(
            "https://tasks.example.com",
            None,
            "x".repeat(400),
            Some("cc_0123456789abcd".into()),
        )
        .unwrap_err();
        assert!(err.to_string().contains("exceeds the 250 byte budget"));
    }
}
