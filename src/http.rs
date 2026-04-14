use std::fmt;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration as StdDuration, SystemTime};

use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore};
use serde::{Deserialize, Serialize};
use time::{Date, Month, PrimitiveDateTime, Time};

#[derive(Debug, Clone)]
pub struct AdminClient {
    base_url: String,
    client: Client,
    token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminApiErrorKind {
    Network,
    Tls,
    Unauthorized,
    Forbidden,
    NotFound,
    PreconditionFailed,
    ServiceUnavailable,
    InvalidRequest,
    Decode,
    OtherStatus(u16),
}

#[derive(Debug, Clone)]
pub struct AdminApiError {
    kind: AdminApiErrorKind,
    code: Option<String>,
    message: String,
}

impl AdminApiError {
    fn new(kind: AdminApiErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            code: None,
            message: message.into(),
        }
    }

    fn new_with_code(
        kind: AdminApiErrorKind,
        code: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            code,
            message: message.into(),
        }
    }

    fn network(message: impl Into<String>) -> Self {
        Self::new(AdminApiErrorKind::Network, message)
    }

    fn tls(message: impl Into<String>) -> Self {
        Self::new(AdminApiErrorKind::Tls, message)
    }

    fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(AdminApiErrorKind::InvalidRequest, message)
    }

    fn decode(message: impl Into<String>) -> Self {
        Self::new(AdminApiErrorKind::Decode, message)
    }

    pub fn is_not_found(&self) -> bool {
        matches!(self.kind, AdminApiErrorKind::NotFound)
    }

    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }
}

impl fmt::Display for AdminApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AdminApiError {}

type AdminApiResult<T> = std::result::Result<T, AdminApiError>;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub pending_tasks: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerStatusResponse {
    pub status: String,
    #[serde(alias = "uptimeSeconds")]
    pub uptime_seconds: f64,
    #[serde(alias = "cachedReplicas")]
    pub cached_replicas: usize,
    #[serde(alias = "quarantinedUsers")]
    pub quarantined_users: usize,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    #[serde(alias = "dataDirUsedBytes")]
    pub data_dir_used_bytes: Option<u64>,
    #[serde(default)]
    #[serde(alias = "authCacheSize")]
    pub auth_cache_size: Option<String>,
    #[serde(default)]
    #[serde(alias = "configDb")]
    pub config_db: Option<String>,
    #[serde(default)]
    #[serde(alias = "llmCircuitBreaker")]
    pub llm_circuit_breaker: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapRequest {
    pub device_name: String,
    pub bootstrap_request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub create_user_if_missing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_server_url_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapResponse {
    pub user_id: String,
    pub username: String,
    pub canonical_client_id: String,
    pub device_client_id: String,
    pub encryption_secret: String,
    pub server_url: String,
    pub taskrc_lines: Vec<String>,
    pub bootstrap_status: String,
    pub created_user: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConnectConfigRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConnectConfigResponse {
    pub credential: String,
    pub token_id: String,
    pub server_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserSummary {
    pub id: String,
    pub username: String,
    pub created_at: String,
    pub device_count: usize,
    pub last_sync_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRecoveryAssessment {
    pub status: String,
    pub device_count: usize,
    pub active_device_count: usize,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserStatsResponse {
    pub user_id: String,
    pub replica_cached: bool,
    pub task_count: Option<usize>,
    pub pending_count: Option<usize>,
    pub replica_dir_exists: bool,
    pub replica_dir_size_bytes: Option<u64>,
    pub quarantined: bool,
    pub recovery_assessment: UserRecoveryAssessment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteUserResponse {
    pub user_id: String,
    pub username: String,
    pub device_count_removed: usize,
    pub replica_dir_removed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatorDeviceResponse {
    pub client_id: String,
    pub name: String,
    pub registered_at: String,
    pub last_sync_at: Option<String>,
    pub last_sync_ip: Option<String>,
    pub status: String,
    pub bootstrap_request_id: Option<String>,
    pub bootstrap_status: Option<String>,
    pub bootstrap_expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminWebhookSummary {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
    pub modified_fields: Option<Vec<String>>,
    pub name: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub consecutive_failures: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminWebhookDelivery {
    pub delivery_id: String,
    pub event_id: String,
    pub event: String,
    pub timestamp: String,
    pub status: String,
    pub response_status: Option<u16>,
    pub attempt: u32,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminWebhookDetail {
    #[serde(flatten)]
    pub webhook: AdminWebhookSummary,
    pub deliveries: Vec<AdminWebhookDelivery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminWebhookTestResponse {
    pub delivery: AdminWebhookDelivery,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAdminWebhookRequest {
    pub url: String,
    pub secret: String,
    pub events: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_fields: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAdminWebhookRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupCreateResponse {
    pub timestamp: String,
    pub path: String,
    pub users: usize,
    pub total_size_bytes: u64,
    pub secrets_included: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummaryResponse {
    pub timestamp: String,
    pub server_version: String,
    pub users: usize,
    pub task_count: Option<u64>,
    pub total_size_bytes: u64,
    pub secrets_included: bool,
    pub backup_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupListResponse {
    backups: Vec<BackupSummaryResponse>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupRestoreRequest {
    timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestoreReplicaResponse {
    pub user_id: String,
    pub username: String,
    pub task_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestoreResponse {
    pub restored_from: String,
    pub pre_restore_snapshot: String,
    pub users_restored: usize,
    pub replicas_restored: usize,
    pub secrets_restored: bool,
    pub config_database_restored: bool,
    pub replicas: Vec<BackupRestoreReplicaResponse>,
}

#[derive(Debug, Deserialize)]
struct AdminErrorResponse {
    code: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TlsCertificateInfo {
    pub not_after: SystemTime,
    pub expires_in: Option<StdDuration>,
}

impl AdminClient {
    pub fn new(base_url: String, token: String) -> AdminApiResult<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).map_err(|err| {
                AdminApiError::invalid_request(format!(
                    "invalid admin token for Authorization header: {err}"
                ))
            })?,
        );

        let client = Client::builder()
            .default_headers(headers)
            .user_agent(format!("cmdock-admin/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|err| AdminApiError::network(format!("failed to build HTTP client: {err}")))?;

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
            token,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn health(&self) -> AdminApiResult<HealthResponse> {
        self.get_json("/healthz")
    }

    pub fn admin_status(&self) -> AdminApiResult<ServerStatusResponse> {
        self.get_json("/admin/status")
    }

    pub fn list_users(&self) -> AdminApiResult<Vec<AdminUserSummary>> {
        self.get_json("/admin/users")
    }

    pub fn user_stats(&self, user_id: &str) -> AdminApiResult<UserStatsResponse> {
        self.get_json(&format!("/admin/user/{user_id}/stats"))
    }

    pub fn delete_user(&self, user_id: &str) -> AdminApiResult<DeleteUserResponse> {
        let url = format!("{}/admin/user/{user_id}", self.base_url);
        let resp = self.client.delete(url).send().map_err(|err| {
            AdminApiError::network(format!("failed to call delete-user endpoint: {err}"))
        })?;
        parse_json(resp)
    }

    pub fn list_user_devices(&self, user_id: &str) -> AdminApiResult<Vec<OperatorDeviceResponse>> {
        self.get_json(&format!("/admin/user/{user_id}/devices"))
    }

    pub fn revoke_user_device(&self, user_id: &str, client_id: &str) -> AdminApiResult<()> {
        let url = format!(
            "{}/admin/user/{user_id}/devices/{client_id}/revoke",
            self.base_url
        );
        let resp = self.client.post(url).send().map_err(|err| {
            AdminApiError::network(format!("failed to call revoke-device endpoint: {err}"))
        })?;
        parse_empty(resp)
    }

    pub fn list_admin_webhooks(&self) -> AdminApiResult<Vec<AdminWebhookSummary>> {
        self.get_json("/admin/webhooks")
    }

    pub fn get_admin_webhook(&self, webhook_id: &str) -> AdminApiResult<AdminWebhookDetail> {
        self.get_json(&format!("/admin/webhooks/{webhook_id}"))
    }

    pub fn create_admin_webhook(
        &self,
        req: &CreateAdminWebhookRequest,
    ) -> AdminApiResult<AdminWebhookSummary> {
        let url = format!("{}/admin/webhooks", self.base_url);
        let resp = self.client.post(url).json(req).send().map_err(|err| {
            AdminApiError::network(format!("failed to call create-webhook endpoint: {err}"))
        })?;
        parse_json(resp)
    }

    pub fn update_admin_webhook(
        &self,
        webhook_id: &str,
        req: &UpdateAdminWebhookRequest,
    ) -> AdminApiResult<AdminWebhookSummary> {
        let url = format!("{}/admin/webhooks/{webhook_id}", self.base_url);
        let resp = self.client.patch(url).json(req).send().map_err(|err| {
            AdminApiError::network(format!("failed to call update-webhook endpoint: {err}"))
        })?;
        parse_json(resp)
    }

    pub fn delete_admin_webhook(&self, webhook_id: &str) -> AdminApiResult<()> {
        let url = format!("{}/admin/webhooks/{webhook_id}", self.base_url);
        let resp = self.client.delete(url).send().map_err(|err| {
            AdminApiError::network(format!("failed to call delete-webhook endpoint: {err}"))
        })?;
        parse_empty(resp)
    }

    pub fn test_admin_webhook(&self, webhook_id: &str) -> AdminApiResult<AdminWebhookTestResponse> {
        let url = format!("{}/admin/webhooks/{webhook_id}/test", self.base_url);
        let resp = self.client.post(url).send().map_err(|err| {
            AdminApiError::network(format!("failed to call test-webhook endpoint: {err}"))
        })?;
        parse_json(resp)
    }

    pub fn create_backup(&self, include_secrets: bool) -> AdminApiResult<BackupCreateResponse> {
        let url = format!(
            "{}/admin/backup?include_secrets={}",
            self.base_url, include_secrets
        );
        let resp = self.client.post(url).send().map_err(|err| {
            AdminApiError::network(format!("failed to call backup endpoint: {err}"))
        })?;
        parse_json(resp)
    }

    pub fn list_backups(&self) -> AdminApiResult<Vec<BackupSummaryResponse>> {
        let response: BackupListResponse = self.get_json("/admin/backup/list")?;
        Ok(response.backups)
    }

    pub fn restore_backup(&self, timestamp: &str) -> AdminApiResult<BackupRestoreResponse> {
        let url = format!("{}/admin/backup/restore", self.base_url);
        let resp = self
            .client
            .post(url)
            .json(&BackupRestoreRequest {
                timestamp: timestamp.to_string(),
            })
            .send()
            .map_err(|err| {
                AdminApiError::network(format!("failed to call restore endpoint: {err}"))
            })?;
        parse_json(resp)
    }

    pub fn bootstrap_user_device(
        &self,
        req: &BootstrapRequest,
    ) -> AdminApiResult<BootstrapResponse> {
        let url = format!("{}/admin/bootstrap/user-device", self.base_url);
        let resp = self.client.post(url).json(req).send().map_err(|err| {
            AdminApiError::network(format!("failed to call bootstrap endpoint: {err}"))
        })?;
        parse_json(resp)
    }

    pub fn create_connect_config(
        &self,
        user_id: &str,
        req: &CreateConnectConfigRequest,
    ) -> AdminApiResult<CreateConnectConfigResponse> {
        let url = format!("{}/admin/user/{user_id}/connect-config", self.base_url);
        let resp = self.client.post(url).json(req).send().map_err(|err| {
            AdminApiError::network(format!("failed to call connect-config endpoint: {err}"))
        })?;
        parse_json(resp)
    }

    pub fn tls_certificate_info(&self) -> AdminApiResult<TlsCertificateInfo> {
        let url = reqwest::Url::parse(&self.base_url).map_err(|err| {
            AdminApiError::invalid_request(format!("invalid server URL '{}': {err}", self.base_url))
        })?;
        if url.scheme() != "https" {
            return Err(AdminApiError::tls(
                "TLS certificate inspection requires an https server URL",
            ));
        }

        let host = url.host_str().ok_or_else(|| {
            AdminApiError::invalid_request(format!(
                "server URL '{}' is missing a hostname",
                self.base_url
            ))
        })?;
        let port = url.port_or_known_default().unwrap_or(443);
        let addr = (host, port)
            .to_socket_addrs()
            .map_err(|err| {
                AdminApiError::network(format!(
                    "failed to resolve {host}:{port} for TLS inspection: {err}"
                ))
            })?
            .next()
            .ok_or_else(|| {
                AdminApiError::network(format!(
                    "no socket address resolved for {host}:{port} during TLS inspection"
                ))
            })?;

        let mut stream =
            TcpStream::connect_timeout(&addr, StdDuration::from_secs(5)).map_err(|err| {
                AdminApiError::network(format!(
                    "failed to connect to {host}:{port} for TLS inspection: {err}"
                ))
            })?;
        let _ = stream.set_read_timeout(Some(StdDuration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(StdDuration::from_secs(5)));

        let native = rustls_native_certs::load_native_certs();
        let mut roots = RootCertStore::empty();
        for cert in native.certs {
            roots.add(cert).map_err(|err| {
                AdminApiError::tls(format!("failed to add a native root certificate: {err}"))
            })?;
        }
        if roots.is_empty() {
            return Err(AdminApiError::tls(
                "no native root certificates were available for TLS verification",
            ));
        }

        let config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let server_name = ServerName::try_from(host.to_string()).map_err(|_| {
            AdminApiError::invalid_request(format!(
                "server hostname '{host}' is not valid for TLS SNI"
            ))
        })?;
        let mut conn = ClientConnection::new(Arc::new(config), server_name)
            .map_err(|err| AdminApiError::tls(format!("failed to start TLS handshake: {err}")))?;

        while conn.is_handshaking() {
            conn.complete_io(&mut stream).map_err(|err| {
                AdminApiError::tls(format!("TLS handshake failed for {host}:{port}: {err}"))
            })?;
        }

        let certificates = conn
            .peer_certificates()
            .ok_or_else(|| AdminApiError::tls("server did not present a TLS certificate"))?;
        let leaf = certificates
            .first()
            .ok_or_else(|| AdminApiError::tls("server presented an empty TLS certificate chain"))?;
        let not_after = parse_certificate_not_after(leaf.as_ref())?;

        Ok(TlsCertificateInfo {
            not_after,
            expires_in: not_after.duration_since(SystemTime::now()).ok(),
        })
    }
}

impl AdminClient {
    fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> AdminApiResult<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(url)
            .send()
            .map_err(|err| AdminApiError::network(format!("failed to GET {path}: {err}")))?;
        parse_json(resp)
    }
}

fn parse_json<T: for<'de> Deserialize<'de>>(resp: Response) -> AdminApiResult<T> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_else(|_| "<no body>".to_string());
        let trimmed = body.trim();
        let detail = if trimmed.is_empty() {
            "<no body>"
        } else {
            trimmed
        };
        let structured = serde_json::from_str::<AdminErrorResponse>(trimmed).ok();
        let code = structured.as_ref().and_then(|payload| payload.code.clone());
        let detail = structured
            .as_ref()
            .and_then(|payload| payload.message.as_deref())
            .filter(|message| !message.trim().is_empty())
            .unwrap_or(detail);
        let (kind, message) = match status {
            StatusCode::UNAUTHORIZED => (
                AdminApiErrorKind::Unauthorized,
                "admin authentication failed: check that the token matches the server config"
                    .to_string(),
            ),
            StatusCode::NOT_FOUND => (
                AdminApiErrorKind::NotFound,
                if looks_like_missing_endpoint(detail) {
                    format!("server endpoint not found: {detail}")
                } else {
                    format!("server resource not found: {detail}")
                },
            ),
            StatusCode::FORBIDDEN => (
                AdminApiErrorKind::Forbidden,
                format!("server rejected the request: {detail}"),
            ),
            StatusCode::PRECONDITION_FAILED => (
                AdminApiErrorKind::PreconditionFailed,
                format!("server precondition failed: {detail}"),
            ),
            StatusCode::SERVICE_UNAVAILABLE => (
                AdminApiErrorKind::ServiceUnavailable,
                format!("server reported the endpoint is unavailable: {detail}"),
            ),
            StatusCode::BAD_REQUEST => (
                AdminApiErrorKind::InvalidRequest,
                format!("server rejected the request: {detail}"),
            ),
            _ => (
                AdminApiErrorKind::OtherStatus(status.as_u16()),
                format!("server returned {status}: {detail}"),
            ),
        };
        return Err(AdminApiError::new_with_code(kind, code, message));
    }

    resp.json::<T>()
        .map_err(|err| AdminApiError::decode(format!("failed to decode JSON response: {err}")))
}

fn parse_empty(resp: Response) -> AdminApiResult<()> {
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    let body = resp.text().unwrap_or_else(|_| "<no body>".to_string());
    let trimmed = body.trim();
    let detail = if trimmed.is_empty() {
        "<no body>"
    } else {
        trimmed
    };
    let structured = serde_json::from_str::<AdminErrorResponse>(trimmed).ok();
    let code = structured.as_ref().and_then(|payload| payload.code.clone());
    let detail = structured
        .as_ref()
        .and_then(|payload| payload.message.as_deref())
        .filter(|message| !message.trim().is_empty())
        .unwrap_or(detail);
    let (kind, message) = match status {
        StatusCode::UNAUTHORIZED => (
            AdminApiErrorKind::Unauthorized,
            "admin authentication failed: check that the token matches the server config"
                .to_string(),
        ),
        StatusCode::NOT_FOUND => (
            AdminApiErrorKind::NotFound,
            if looks_like_missing_endpoint(detail) {
                format!("server endpoint not found: {detail}")
            } else {
                format!("server resource not found: {detail}")
            },
        ),
        StatusCode::FORBIDDEN => (
            AdminApiErrorKind::Forbidden,
            format!("server rejected the request: {detail}"),
        ),
        StatusCode::PRECONDITION_FAILED => (
            AdminApiErrorKind::PreconditionFailed,
            format!("server precondition failed: {detail}"),
        ),
        StatusCode::SERVICE_UNAVAILABLE => (
            AdminApiErrorKind::ServiceUnavailable,
            format!("server reported the endpoint is unavailable: {detail}"),
        ),
        StatusCode::BAD_REQUEST => (
            AdminApiErrorKind::InvalidRequest,
            format!("server rejected the request: {detail}"),
        ),
        _ => (
            AdminApiErrorKind::OtherStatus(status.as_u16()),
            format!("server returned {status}: {detail}"),
        ),
    };
    Err(AdminApiError::new_with_code(kind, code, message))
}

fn looks_like_missing_endpoint(detail: &str) -> bool {
    matches!(detail, "<no body>" | "Not Found" | "404 page not found")
}

fn parse_certificate_not_after(cert_der: &[u8]) -> AdminApiResult<SystemTime> {
    let mut cert_cursor = 0usize;
    let certificate = read_tlv_value(cert_der, &mut cert_cursor, 0x30)?;

    let mut certificate_cursor = 0usize;
    let tbs_certificate = read_tlv_value(certificate, &mut certificate_cursor, 0x30)?;

    let mut tbs_cursor = 0usize;
    if peek_tag(tbs_certificate, tbs_cursor) == Some(0xa0) {
        skip_tlv(tbs_certificate, &mut tbs_cursor)?;
    }
    skip_tlv(tbs_certificate, &mut tbs_cursor)?;
    skip_tlv(tbs_certificate, &mut tbs_cursor)?;
    skip_tlv(tbs_certificate, &mut tbs_cursor)?;

    let validity = read_tlv_value(tbs_certificate, &mut tbs_cursor, 0x30)?;
    let mut validity_cursor = 0usize;
    skip_tlv(validity, &mut validity_cursor)?;
    let (tag, value) = read_tlv(validity, &mut validity_cursor)?;
    parse_certificate_time(tag, value)
}

fn peek_tag(input: &[u8], cursor: usize) -> Option<u8> {
    input.get(cursor).copied()
}

fn skip_tlv(input: &[u8], cursor: &mut usize) -> AdminApiResult<()> {
    let _ = read_tlv(input, cursor)?;
    Ok(())
}

fn read_tlv_value<'a>(
    input: &'a [u8],
    cursor: &mut usize,
    expected_tag: u8,
) -> AdminApiResult<&'a [u8]> {
    let (tag, value) = read_tlv(input, cursor)?;
    if tag != expected_tag {
        return Err(AdminApiError::decode(format!(
            "unexpected DER tag 0x{tag:02x}; expected 0x{expected_tag:02x}"
        )));
    }
    Ok(value)
}

fn read_tlv<'a>(input: &'a [u8], cursor: &mut usize) -> AdminApiResult<(u8, &'a [u8])> {
    let tag = *input
        .get(*cursor)
        .ok_or_else(|| AdminApiError::decode("unexpected end of DER input"))?;
    *cursor += 1;

    let length = read_der_length(input, cursor)?;
    let end = cursor
        .checked_add(length)
        .ok_or_else(|| AdminApiError::decode("DER length overflow"))?;
    let value = input
        .get(*cursor..end)
        .ok_or_else(|| AdminApiError::decode("DER length exceeded certificate size"))?;
    *cursor = end;
    Ok((tag, value))
}

fn read_der_length(input: &[u8], cursor: &mut usize) -> AdminApiResult<usize> {
    let first = *input
        .get(*cursor)
        .ok_or_else(|| AdminApiError::decode("unexpected end of DER length"))?;
    *cursor += 1;

    if first & 0x80 == 0 {
        return Ok(first as usize);
    }

    let octets = (first & 0x7f) as usize;
    if octets == 0 {
        return Err(AdminApiError::decode(
            "indefinite DER lengths are not supported in TLS certificates",
        ));
    }
    if octets > std::mem::size_of::<usize>() {
        return Err(AdminApiError::decode("DER length is too large to decode"));
    }

    let mut length = 0usize;
    for _ in 0..octets {
        let byte = *input
            .get(*cursor)
            .ok_or_else(|| AdminApiError::decode("unexpected end of DER length"))?;
        *cursor += 1;
        length = (length << 8) | byte as usize;
    }
    Ok(length)
}

fn parse_certificate_time(tag: u8, value: &[u8]) -> AdminApiResult<SystemTime> {
    let raw = std::str::from_utf8(value).map_err(|err| {
        AdminApiError::decode(format!("certificate time was not valid UTF-8: {err}"))
    })?;
    let (year, month, day, hour, minute, second) = match tag {
        0x17 => parse_utc_time(raw)?,
        0x18 => parse_generalized_time(raw)?,
        other => {
            return Err(AdminApiError::decode(format!(
                "unexpected certificate time tag 0x{other:02x}"
            )));
        }
    };

    let month = Month::try_from(month)
        .map_err(|_| AdminApiError::decode(format!("invalid certificate month: {month}")))?;
    let date = Date::from_calendar_date(year, month, day)
        .map_err(|err| AdminApiError::decode(format!("invalid certificate expiry date: {err}")))?;
    let time = Time::from_hms(hour, minute, second)
        .map_err(|err| AdminApiError::decode(format!("invalid certificate expiry time: {err}")))?;
    let datetime = PrimitiveDateTime::new(date, time).assume_utc();
    Ok(SystemTime::from(datetime))
}

fn parse_utc_time(raw: &str) -> AdminApiResult<(i32, u8, u8, u8, u8, u8)> {
    if raw.len() != 13 || !raw.ends_with('Z') {
        return Err(AdminApiError::decode(format!(
            "invalid UTCTime value in certificate: {raw}"
        )));
    }

    let year = parse_u32_component(raw, 0, 2)? as i32;
    let year = if year >= 50 { 1900 + year } else { 2000 + year };
    Ok((
        year,
        parse_u32_component(raw, 2, 2)? as u8,
        parse_u32_component(raw, 4, 2)? as u8,
        parse_u32_component(raw, 6, 2)? as u8,
        parse_u32_component(raw, 8, 2)? as u8,
        parse_u32_component(raw, 10, 2)? as u8,
    ))
}

fn parse_generalized_time(raw: &str) -> AdminApiResult<(i32, u8, u8, u8, u8, u8)> {
    if raw.len() != 15 || !raw.ends_with('Z') {
        return Err(AdminApiError::decode(format!(
            "invalid GeneralizedTime value in certificate: {raw}"
        )));
    }

    Ok((
        parse_u32_component(raw, 0, 4)? as i32,
        parse_u32_component(raw, 4, 2)? as u8,
        parse_u32_component(raw, 6, 2)? as u8,
        parse_u32_component(raw, 8, 2)? as u8,
        parse_u32_component(raw, 10, 2)? as u8,
        parse_u32_component(raw, 12, 2)? as u8,
    ))
}

fn parse_u32_component(raw: &str, start: usize, len: usize) -> AdminApiResult<u32> {
    raw.get(start..start + len)
        .ok_or_else(|| AdminApiError::decode(format!("invalid certificate time value: {raw}")))?
        .parse::<u32>()
        .map_err(|err| {
            AdminApiError::decode(format!(
                "invalid numeric component in certificate time '{raw}': {err}"
            ))
        })
}
