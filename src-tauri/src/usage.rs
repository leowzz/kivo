use crate::{storage::atomic_write, workspace::AppError};
use reqwest::{
    StatusCode, Url,
    blocking::{Client, Response},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const DOCUMENT_SCHEMA_VERSION: u16 = 1;
const TOKEN_REFRESH_MARGIN_SECONDS: i64 = 120;
const RETRY_INTERVAL: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageState {
    #[default]
    Disabled,
    Connecting,
    Ready,
    Stale,
    AuthError,
    NetworkError,
    ParseError,
    ApiError,
}

impl UsageState {
    pub(crate) fn protocol_code(self) -> u8 {
        match self {
            Self::Disabled => 0,
            Self::Connecting => 1,
            Self::Ready => 2,
            Self::Stale => 3,
            Self::AuthError => 4,
            Self::NetworkError => 5,
            Self::ParseError => 6,
            Self::ApiError => 7,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub state: UsageState,
    pub has_data: bool,
    pub cost_micros: u64,
    pub today_tokens: u64,
    pub tpm: u64,
    pub updated_at_ms: Option<u64>,
}

impl UsageSnapshot {
    fn failure(state: UsageState, previous: &Self) -> Self {
        if previous.has_data {
            Self {
                state: UsageState::Stale,
                ..previous.clone()
            }
        } else {
            Self {
                state,
                ..Self::default()
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSettingsSummary {
    pub enabled: bool,
    pub base_url: String,
    pub email: String,
    pub interval_seconds: u64,
    pub password_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageView {
    pub settings: UsageSettingsSummary,
    pub snapshot: UsageSnapshot,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UsageSettingsPatch {
    pub enabled: bool,
    pub base_url: String,
    pub email: String,
    #[serde(default)]
    pub password: String,
    pub interval_seconds: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct PersistedSettings {
    enabled: bool,
    base_url: String,
    email: String,
    interval_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct TokenPair {
    access_token: String,
    refresh_token: String,
    expires_at_epoch: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedDocument {
    schema_version: u16,
    settings: PersistedSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session: Option<TokenPair>,
}

impl Default for PersistedDocument {
    fn default() -> Self {
        Self {
            schema_version: DOCUMENT_SCHEMA_VERSION,
            settings: PersistedSettings {
                interval_seconds: 60,
                ..PersistedSettings::default()
            },
            session: None,
        }
    }
}

enum UsageCommand {
    Reload { password: Option<String> },
    SetActive(bool),
}

pub struct UsageService {
    document: Arc<Mutex<PersistedDocument>>,
    snapshot: Arc<RwLock<UsageSnapshot>>,
    commands: mpsc::Sender<UsageCommand>,
    path: PathBuf,
}

impl UsageService {
    pub fn spawn(
        app_data_directory: &Path,
        stop: Arc<AtomicBool>,
        updates: mpsc::Sender<Arc<UsageSnapshot>>,
    ) -> Result<(Arc<Self>, JoinHandle<()>), AppError> {
        let directory = app_data_directory.join("usage");
        fs::create_dir_all(&directory).map_err(|error| {
            AppError::new("usage_storage_unavailable").with_detail(error.to_string())
        })?;
        let path = directory.join("sub2api.json");
        let document = load_document(&path)?;
        let initial = if document.settings.enabled {
            UsageSnapshot {
                state: UsageState::Connecting,
                ..UsageSnapshot::default()
            }
        } else {
            UsageSnapshot::default()
        };
        let document = Arc::new(Mutex::new(document));
        let snapshot = Arc::new(RwLock::new(initial));
        let (commands, receiver) = mpsc::channel();
        let service = Arc::new(Self {
            document: Arc::clone(&document),
            snapshot: Arc::clone(&snapshot),
            commands,
            path: path.clone(),
        });
        let thread =
            thread::spawn(move || run_service(document, snapshot, path, receiver, updates, stop));
        Ok((service, thread))
    }

    pub fn view(&self) -> UsageView {
        let document = self
            .document
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let snapshot = self
            .snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        UsageView {
            settings: settings_summary(&document),
            snapshot,
        }
    }

    pub fn save(&self, patch: UsageSettingsPatch) -> Result<UsageView, AppError> {
        let base_url = validate_base_url(&patch.base_url)?;
        let email = patch.email.trim().to_owned();
        if patch.enabled && (email.is_empty() || email.len() > 254) {
            return Err(AppError::new("usage_email_invalid"));
        }
        if !(2..=3600).contains(&patch.interval_seconds) {
            return Err(AppError::new("usage_interval_invalid"));
        }

        let password = (!patch.password.is_empty()).then_some(patch.password);
        {
            let mut document = self
                .document
                .lock()
                .map_err(|_| AppError::new("usage_settings_unavailable"))?;
            let identity_changed =
                document.settings.base_url != base_url || document.settings.email != email;
            if patch.enabled
                && (identity_changed || document.session.is_none())
                && password.is_none()
            {
                return Err(AppError::new("usage_password_required"));
            }
            document.settings = PersistedSettings {
                enabled: patch.enabled,
                base_url,
                email,
                interval_seconds: patch.interval_seconds,
            };
            if identity_changed || !patch.enabled {
                document.session = None;
            }
            persist_document(&self.path, &document)?;
        }

        let next = if patch.enabled {
            UsageSnapshot {
                state: UsageState::Connecting,
                ..self
                    .snapshot
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone()
            }
        } else {
            UsageSnapshot::default()
        };
        *self
            .snapshot
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = next;
        self.commands
            .send(UsageCommand::Reload { password })
            .map_err(|_| AppError::new("usage_service_unavailable"))?;
        Ok(self.view())
    }

    pub fn set_active(&self, active: bool) -> Result<(), AppError> {
        self.commands
            .send(UsageCommand::SetActive(active))
            .map_err(|_| AppError::new("usage_service_unavailable"))
    }
}

fn settings_summary(document: &PersistedDocument) -> UsageSettingsSummary {
    UsageSettingsSummary {
        enabled: document.settings.enabled,
        base_url: document.settings.base_url.clone(),
        email: document.settings.email.clone(),
        interval_seconds: document.settings.interval_seconds,
        password_required: document.settings.enabled && document.session.is_none(),
    }
}

fn validate_base_url(value: &str) -> Result<String, AppError> {
    let value = value.trim().trim_end_matches('/');
    if value.is_empty() {
        return Ok(String::new());
    }
    let parsed = Url::parse(value).map_err(|_| AppError::new("usage_base_url_invalid"))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path() != "/"
    {
        return Err(AppError::new("usage_base_url_invalid"));
    }
    Ok(value.to_owned())
}

fn load_document(path: &Path) -> Result<PersistedDocument, AppError> {
    if !path.exists() {
        return Ok(PersistedDocument::default());
    }
    let bytes = fs::read(path).map_err(|error| {
        AppError::new("usage_settings_unavailable").with_detail(error.to_string())
    })?;
    let document: PersistedDocument = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::new("usage_settings_invalid").with_detail(error.to_string()))?;
    if document.schema_version != DOCUMENT_SCHEMA_VERSION {
        return Err(AppError::new("usage_settings_schema_unsupported"));
    }
    validate_base_url(&document.settings.base_url)?;
    Ok(document)
}

fn persist_document(path: &Path, document: &PersistedDocument) -> Result<(), AppError> {
    let bytes = serde_json::to_vec_pretty(document).map_err(|error| {
        AppError::new("usage_settings_save_failed").with_detail(error.to_string())
    })?;
    atomic_write(path, &bytes)
        .map_err(|error| AppError::new("usage_settings_save_failed").with_detail(error))
}

fn run_service(
    document: Arc<Mutex<PersistedDocument>>,
    snapshot: Arc<RwLock<UsageSnapshot>>,
    path: PathBuf,
    commands: mpsc::Receiver<UsageCommand>,
    updates: mpsc::Sender<Arc<UsageSnapshot>>,
    stop: Arc<AtomicBool>,
) {
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(Policy::none())
        .build()
        .ok();
    let mut password = None;
    let mut wait = Duration::ZERO;
    let mut active = false;

    while !stop.load(Ordering::Relaxed) {
        let command_wait = if active {
            wait.min(Duration::from_millis(500))
        } else {
            Duration::from_millis(500)
        };
        match commands.recv_timeout(command_wait) {
            Ok(UsageCommand::Reload { password: next }) => {
                password = next;
                wait = Duration::ZERO;
            }
            Ok(UsageCommand::SetActive(next)) => {
                active = next;
                wait = Duration::ZERO;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) if !active => continue,
            Err(mpsc::RecvTimeoutError::Timeout) if wait > Duration::from_millis(500) => {
                wait -= Duration::from_millis(500);
                continue;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if !active {
            continue;
        }
        if wait > Duration::ZERO {
            wait = Duration::ZERO;
            continue;
        }

        let current_document = document
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if !current_document.settings.enabled {
            publish(&snapshot, &updates, UsageSnapshot::default());
            wait = Duration::from_secs(3600);
            continue;
        }
        let previous = snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let Some(client) = client.as_ref() else {
            publish(
                &snapshot,
                &updates,
                UsageSnapshot::failure(UsageState::NetworkError, &previous),
            );
            wait = RETRY_INTERVAL;
            continue;
        };

        match fetch_usage(client, &current_document, password.take()) {
            Ok(result) => {
                let settings_unchanged = {
                    let mut active = document
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if active.settings != current_document.settings {
                        false
                    } else {
                        if active.session.as_ref() != Some(&result.session) {
                            active.session = Some(result.session);
                            let _ = persist_document(&path, &active);
                        }
                        true
                    }
                };
                if settings_unchanged {
                    publish(&snapshot, &updates, result.snapshot);
                    wait = Duration::from_secs(current_document.settings.interval_seconds);
                }
            }
            Err(kind) => {
                publish(
                    &snapshot,
                    &updates,
                    UsageSnapshot::failure(kind.state(), &previous),
                );
                wait = RETRY_INTERVAL;
            }
        }
    }
}

fn publish(
    shared: &RwLock<UsageSnapshot>,
    updates: &mpsc::Sender<Arc<UsageSnapshot>>,
    snapshot: UsageSnapshot,
) {
    *shared
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = snapshot.clone();
    let _ = updates.send(Arc::new(snapshot));
}

struct FetchResult {
    session: TokenPair,
    snapshot: UsageSnapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FetchError {
    Auth,
    Network,
    Parse,
    Api,
}

impl FetchError {
    fn state(self) -> UsageState {
        match self {
            Self::Auth => UsageState::AuthError,
            Self::Network => UsageState::NetworkError,
            Self::Parse => UsageState::ParseError,
            Self::Api => UsageState::ApiError,
        }
    }
}

fn fetch_usage(
    client: &Client,
    document: &PersistedDocument,
    password: Option<String>,
) -> Result<FetchResult, FetchError> {
    let now = now_epoch();
    let mut session = match document.session.clone() {
        Some(session) if session.expires_at_epoch > now + TOKEN_REFRESH_MARGIN_SECONDS => session,
        Some(session) => refresh(
            client,
            &document.settings.base_url,
            &session.refresh_token,
            now,
        )
        .or_else(|_| login_with_password(client, document, password.as_deref(), now))?,
        None => login_with_password(client, document, password.as_deref(), now)?,
    };

    let reading = match request_usage(client, &document.settings.base_url, &session.access_token) {
        Err(FetchError::Auth) => {
            session = refresh(
                client,
                &document.settings.base_url,
                &session.refresh_token,
                now,
            )
            .or_else(|_| login_with_password(client, document, password.as_deref(), now))?;
            request_usage(client, &document.settings.base_url, &session.access_token)?
        }
        result => result?,
    };
    Ok(FetchResult {
        session,
        snapshot: UsageSnapshot {
            state: UsageState::Ready,
            has_data: true,
            cost_micros: reading.cost_micros,
            today_tokens: reading.today_tokens,
            tpm: reading.tpm,
            updated_at_ms: Some(now_ms()),
        },
    })
}

fn login_with_password(
    client: &Client,
    document: &PersistedDocument,
    password: Option<&str>,
    now: i64,
) -> Result<TokenPair, FetchError> {
    let password = password
        .filter(|value| !value.is_empty())
        .ok_or(FetchError::Auth)?;
    let response = client
        .post(endpoint(&document.settings.base_url, "/api/v1/auth/login"))
        .json(&serde_json::json!({"email": document.settings.email, "password": password}))
        .send()
        .map_err(|_| FetchError::Network)?;
    parse_token_response(response, now)
}

fn refresh(
    client: &Client,
    base_url: &str,
    refresh_token: &str,
    now: i64,
) -> Result<TokenPair, FetchError> {
    let response = client
        .post(endpoint(base_url, "/api/v1/auth/refresh"))
        .json(&serde_json::json!({"refresh_token": refresh_token}))
        .send()
        .map_err(|_| FetchError::Network)?;
    parse_token_response(response, now)
}

fn request_usage(client: &Client, base_url: &str, token: &str) -> Result<UsageReading, FetchError> {
    let response = client
        .get(endpoint(base_url, "/api/v1/usage/dashboard/stats"))
        .bearer_auth(token)
        .send()
        .map_err(|_| FetchError::Network)?;
    if response.status() == StatusCode::UNAUTHORIZED {
        return Err(FetchError::Auth);
    }
    let data = response_data(response)?;
    parse_usage_data(&data)
}

fn parse_token_response(response: Response, now: i64) -> Result<TokenPair, FetchError> {
    if response.status() == StatusCode::UNAUTHORIZED {
        return Err(FetchError::Auth);
    }
    let data = response_data(response)?;
    let access_token = data
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(FetchError::Parse)?;
    let refresh_token = data
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(FetchError::Parse)?;
    let expires_in = data
        .get("expires_in")
        .and_then(Value::as_i64)
        .filter(|value| *value >= 0)
        .ok_or(FetchError::Parse)?;
    Ok(TokenPair {
        access_token: access_token.to_owned(),
        refresh_token: refresh_token.to_owned(),
        expires_at_epoch: now.checked_add(expires_in).ok_or(FetchError::Parse)?,
    })
}

fn response_data(response: Response) -> Result<Value, FetchError> {
    if !response.status().is_success() {
        return Err(FetchError::Api);
    }
    let body: Value = response.json().map_err(|_| FetchError::Parse)?;
    if body.get("code").and_then(Value::as_i64) != Some(0)
        || body.get("message").and_then(Value::as_str).is_none()
    {
        return Err(FetchError::Api);
    }
    body.get("data")
        .filter(|data| data.is_object())
        .cloned()
        .ok_or(FetchError::Parse)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UsageReading {
    cost_micros: u64,
    today_tokens: u64,
    tpm: u64,
}

fn parse_usage_data(data: &Value) -> Result<UsageReading, FetchError> {
    let cost = data
        .get("today_actual_cost")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or(FetchError::Parse)?;
    let cost_micros = (cost * 1_000_000.0).round();
    if cost_micros > u64::MAX as f64 {
        return Err(FetchError::Parse);
    }
    Ok(UsageReading {
        cost_micros: cost_micros as u64,
        today_tokens: data
            .get("today_tokens")
            .and_then(Value::as_u64)
            .ok_or(FetchError::Parse)?,
        tpm: data
            .get("tpm")
            .and_then(Value::as_u64)
            .ok_or(FetchError::Parse)?,
    })
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!("{base_url}{path}")
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https_origin_and_rejects_paths_or_credentials() {
        assert_eq!(
            validate_base_url(" https://sub2api.example.com/ ").unwrap(),
            "https://sub2api.example.com"
        );
        assert!(validate_base_url("http://sub2api.example.com").is_err());
        assert!(validate_base_url("https://user@sub2api.example.com").is_err());
        assert!(validate_base_url("https://sub2api.example.com/prefix").is_err());
    }

    #[test]
    fn parses_strict_usage_fields_into_integer_protocol_values() {
        let reading = parse_usage_data(&serde_json::json!({
            "today_actual_cost": 12.345678,
            "today_tokens": 1_234_567_u64,
            "tpm": 98_765_u64,
        }))
        .unwrap();
        assert_eq!(reading.cost_micros, 12_345_678);
        assert_eq!(reading.today_tokens, 1_234_567);
        assert_eq!(reading.tpm, 98_765);

        assert!(
            parse_usage_data(&serde_json::json!({
                "today_actual_cost": 1.0,
                "today_tokens": 1.5,
                "tpm": 2,
            }))
            .is_err()
        );
    }

    #[test]
    fn stale_snapshot_preserves_last_successful_reading() {
        let ready = UsageSnapshot {
            state: UsageState::Ready,
            has_data: true,
            cost_micros: 125_000,
            today_tokens: 42,
            tpm: 7,
            updated_at_ms: Some(123),
        };
        assert_eq!(
            UsageSnapshot::failure(UsageState::NetworkError, &ready),
            UsageSnapshot {
                state: UsageState::Stale,
                ..ready
            }
        );
    }

    #[test]
    fn stores_settings_in_app_data_without_persisting_the_password() {
        let directory = tempfile::tempdir().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let (updates, _received) = mpsc::channel();
        let (service, thread) =
            UsageService::spawn(directory.path(), Arc::clone(&stop), updates).unwrap();

        service
            .save(UsageSettingsPatch {
                enabled: false,
                base_url: String::new(),
                email: String::new(),
                password: "do-not-store-this".into(),
                interval_seconds: 30,
            })
            .unwrap();

        let path = directory.path().join("usage/sub2api.json");
        let persisted = fs::read_to_string(path).unwrap();
        assert!(persisted.contains("\"interval_seconds\": 30"));
        assert!(!persisted.contains("do-not-store-this"));

        stop.store(true, Ordering::Relaxed);
        thread.join().unwrap();
    }

    #[test]
    fn does_not_fetch_until_sub2api_view_is_active() {
        let directory = tempfile::tempdir().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server_stop = Arc::new(AtomicBool::new(false));
        let server_stop_signal = Arc::clone(&server_stop);
        let server = thread::spawn(move || {
            while !server_stop_signal.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        drop(stream);
                        return;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => return,
                }
            }
        });
        let (updates, _received) = mpsc::channel();
        let (service, thread) =
            UsageService::spawn(directory.path(), Arc::clone(&stop), updates).unwrap();

        service
            .save(UsageSettingsPatch {
                enabled: true,
                base_url: format!("https://{address}"),
                email: "test@example.com".into(),
                password: "test-password".into(),
                interval_seconds: 30,
            })
            .unwrap();

        thread::sleep(Duration::from_millis(600));
        assert_eq!(service.view().snapshot.state, UsageState::Connecting);

        service.set_active(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while service.view().snapshot.state == UsageState::Connecting
            && std::time::Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(service.view().snapshot.state, UsageState::NetworkError);

        stop.store(true, Ordering::Relaxed);
        thread.join().unwrap();
        server_stop.store(true, Ordering::Relaxed);
        server.join().unwrap();
    }
}
