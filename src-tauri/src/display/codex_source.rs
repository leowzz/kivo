use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs::{self, File},
    hash::{DefaultHasher, Hash, Hasher},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime},
};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use serde::Serialize;
use serde_json::{Value, json};

use super::{
    CodexInputNeed, CodexTaskSnapshot, CodexTerminalEvent, SourceHealth,
    codex_events::{
        CodexRolloutCursorState, CodexRolloutEvent, CodexRolloutIndex, parse_rollout_line,
    },
};

const METADATA_POLL_INTERVAL: Duration = Duration::from_secs(2);
const FILESYSTEM_POLL_INTERVAL: Duration = Duration::from_secs(1);
const MIN_PUBLISH_INTERVAL: Duration = Duration::from_millis(200);
const RECENT_ROLLOUT_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const RECOVERY_CHUNK_SIZE: usize = 64 * 1024;
const CURSOR_VERSION: u32 = 1;
const APP_SERVER_RESPONSE_DEADLINE: Duration = Duration::from_secs(1);
const MAX_METADATA_PAGES: usize = 100;
const CONFIRMED_WINDOW_SIZE: u64 = 4096;

#[derive(Clone, Debug)]
pub struct CodexSourceSnapshot {
    pub health: SourceHealth,
    pub tasks: Vec<MergedCodexTask>,
}

impl CodexSourceSnapshot {
    pub fn task(&self, thread_id: &str) -> Option<&MergedCodexTask> {
        self.tasks.iter().find(|task| task.thread_id == thread_id)
    }
}

#[derive(Clone, Debug)]
pub struct MergedCodexTask {
    pub thread_id: String,
    pub name: Option<String>,
    pub cwd: PathBuf,
    pub updated_at: Instant,
    pub running: bool,
    pub input_need: Option<CodexInputNeed>,
    pub system_error: bool,
    pub terminal_event: Option<CodexTerminalEvent>,
    pub terminal_sequence: u64,
}

pub trait CodexTaskReader: Send {
    fn poll_tasks(&mut self, now: Instant) -> Result<CodexSourceSnapshot, &'static str>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChannelHealth {
    Healthy,
    Unavailable,
}

#[derive(Clone, Debug)]
pub struct CodexThreadMetadata {
    pub thread_id: String,
    pub name: Option<String>,
    pub cwd: PathBuf,
    pub rollout_path: Option<PathBuf>,
    pub server_updated_at: u64,
    pub updated_at: Instant,
    pub status: AppServerStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AppServerStatus {
    NotLoaded,
    Idle,
    SystemError,
    Active {
        #[serde(default, rename = "activeFlags")]
        active_flags: BTreeSet<ActiveFlag>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub enum ActiveFlag {
    WaitingOnApproval,
    WaitingOnUserInput,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MetadataFingerprint {
    name: Option<String>,
    cwd: PathBuf,
    rollout_path: Option<PathBuf>,
    status: AppServerStatus,
}

impl From<&CodexThreadMetadata> for MetadataFingerprint {
    fn from(thread: &CodexThreadMetadata) -> Self {
        Self {
            name: thread.name.clone(),
            cwd: thread.cwd.clone(),
            rollout_path: thread.rollout_path.clone(),
            status: thread.status.clone(),
        }
    }
}

#[derive(Default)]
struct MetadataWatermark {
    timestamp: Option<u64>,
    fingerprints: BTreeMap<String, MetadataFingerprint>,
}

impl MetadataWatermark {
    fn select_page(
        &self,
        last_seen: Option<u64>,
        threads: Vec<CodexThreadMetadata>,
        seen_thread_ids: &mut BTreeSet<String>,
    ) -> (Vec<CodexThreadMetadata>, bool) {
        let mut selected = Vec::new();
        let mut reached_older = false;
        for thread in threads {
            if !seen_thread_ids.insert(thread.thread_id.clone()) {
                continue;
            }
            let include = match last_seen {
                None => true,
                Some(last_seen) if thread.server_updated_at > last_seen => true,
                Some(last_seen) if thread.server_updated_at == last_seen => {
                    self.timestamp != Some(last_seen)
                        || self.fingerprints.get(&thread.thread_id)
                            != Some(&MetadataFingerprint::from(&thread))
                }
                Some(_) => {
                    reached_older = true;
                    false
                }
            };
            if include {
                selected.push(thread);
            }
        }
        (selected, reached_older)
    }

    fn commit(&mut self, threads: &[CodexThreadMetadata]) {
        let Some(timestamp) = threads.iter().map(|thread| thread.server_updated_at).max() else {
            return;
        };
        if self.timestamp.is_none_or(|current| timestamp > current) {
            self.timestamp = Some(timestamp);
            self.fingerprints.clear();
        }
        if self.timestamp == Some(timestamp) {
            self.fingerprints.extend(
                threads
                    .iter()
                    .filter(|thread| thread.server_updated_at == timestamp)
                    .map(|thread| (thread.thread_id.clone(), MetadataFingerprint::from(thread))),
            );
        }
    }
}

fn advance_pagination(
    next_cursor: Option<String>,
    seen_cursors: &mut BTreeSet<String>,
    page_count: usize,
) -> Result<Option<String>, &'static str> {
    let Some(cursor) = next_cursor else {
        return Ok(None);
    };
    if page_count >= MAX_METADATA_PAGES {
        return Err("codex_app_server_page_limit");
    }
    if !seen_cursors.insert(cursor.clone()) {
        return Err("codex_app_server_pagination_loop");
    }
    Ok(Some(cursor))
}

pub trait CodexMetadataClient: Send {
    fn codex_home(&self) -> &std::path::Path;
    fn poll_updated(&mut self, last_seen: Option<u64>) -> Result<Vec<CodexThreadMetadata>, String>;
}

pub struct SystemCodexMetadataClient {
    codex_home: PathBuf,
    connection: Option<AppServerConnection>,
    next_request_id: u64,
    watermark: MetadataWatermark,
}

impl SystemCodexMetadataClient {
    pub fn new(codex_home_fallback: impl Into<PathBuf>) -> Self {
        Self {
            codex_home: codex_home_fallback.into(),
            connection: None,
            next_request_id: 1,
            watermark: MetadataWatermark::default(),
        }
    }

    fn is_connected(&self) -> bool {
        self.connection.is_some()
    }

    fn ensure_connected(&mut self) -> Result<(), &'static str> {
        if self.connection.is_some() {
            return Ok(());
        }
        let executable = locate_codex().ok_or("codex_cli_unavailable")?;
        let mut connection = AppServerConnection::spawn(&executable)?;
        let initialize_id = self.take_request_id();
        let initialize = connection.request(
            initialize_id,
            "initialize",
            json!({
                "clientInfo": {
                    "name": "kivo",
                    "title": "Kivo",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )?;
        let codex_home = initialize
            .codex_home
            .as_deref()
            .ok_or("codex_initialize_invalid_response")?;
        self.codex_home = PathBuf::from(codex_home);
        connection.notify("initialized", None)?;
        self.connection = Some(connection);
        Ok(())
    }

    fn take_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        id
    }

    fn disconnect(&mut self) {
        drop(self.connection.take());
    }
}

impl CodexMetadataClient for SystemCodexMetadataClient {
    fn codex_home(&self) -> &Path {
        &self.codex_home
    }

    fn poll_updated(&mut self, last_seen: Option<u64>) -> Result<Vec<CodexThreadMetadata>, String> {
        if let Err(code) = self.ensure_connected() {
            self.disconnect();
            return Err(code.into());
        }
        let mut cursor = None;
        let mut threads = Vec::new();
        let mut seen_cursors = BTreeSet::new();
        let mut seen_thread_ids = BTreeSet::new();
        let mut page_count = 0;
        loop {
            page_count += 1;
            let request_id = self.take_request_id();
            let response = match self
                .connection
                .as_mut()
                .expect("connection initialized")
                .request(
                    request_id,
                    "thread/list",
                    thread_list_params(cursor.as_deref()),
                ) {
                Ok(response) => response,
                Err(code) => {
                    self.disconnect();
                    return Err(code.into());
                }
            };
            let page = match parse_thread_list_result(response, Instant::now()) {
                Ok(page) => page,
                Err(code) => {
                    self.disconnect();
                    return Err(code.into());
                }
            };
            let (selected, reached_older) =
                self.watermark
                    .select_page(last_seen, page.threads, &mut seen_thread_ids);
            threads.extend(selected);
            cursor = match advance_pagination(page.next_cursor, &mut seen_cursors, page_count) {
                Ok(cursor) => cursor,
                Err(code) => {
                    self.disconnect();
                    return Err(code.into());
                }
            };
            if reached_older || cursor.is_none() {
                break;
            }
        }
        self.watermark.commit(&threads);
        Ok(threads)
    }
}

impl Drop for SystemCodexMetadataClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

struct AppServerConnection {
    child: Child,
    stdin: ChildStdin,
    responses: Receiver<AppServerResponse>,
    reader: Option<JoinHandle<()>>,
}

impl AppServerConnection {
    fn spawn(executable: &Path) -> Result<Self, &'static str> {
        let mut child = Command::new(executable)
            .args(["app-server", "--listen", "stdio://"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "codex_app_server_spawn_failed")?;
        let Some(stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("codex_app_server_stdio_failed");
        };
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("codex_app_server_stdio_failed");
        };
        let (response_tx, responses) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                if let Some(response) = route_app_server_line(&line)
                    && response_tx.send(response).is_err()
                {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            responses,
            reader: Some(reader),
        })
    }

    fn request(
        &mut self,
        id: u64,
        method: &'static str,
        params: Value,
    ) -> Result<AppServerResult, &'static str> {
        self.write_message(&json!({"id": id, "method": method, "params": params}))?;
        let deadline = Instant::now() + APP_SERVER_RESPONSE_DEADLINE;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("codex_app_server_timeout");
            }
            match self.responses.recv_timeout(remaining) {
                Ok(response) if response.id == id => {
                    if response.error {
                        return Err("codex_app_server_request_failed");
                    }
                    return response.result.ok_or("codex_app_server_invalid_response");
                }
                Ok(_) => continue,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err("codex_app_server_timeout");
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("codex_app_server_eof");
                }
            }
        }
    }

    fn notify(&mut self, method: &'static str, params: Option<Value>) -> Result<(), &'static str> {
        let message = match params {
            Some(params) => json!({"method": method, "params": params}),
            None => json!({"method": method}),
        };
        self.write_message(&message)
    }

    fn write_message(&mut self, message: &Value) -> Result<(), &'static str> {
        serde_json::to_writer(&mut self.stdin, message)
            .map_err(|_| "codex_app_server_write_failed")?;
        self.stdin
            .write_all(b"\n")
            .and_then(|_| self.stdin.flush())
            .map_err(|_| "codex_app_server_write_failed")
    }

    fn shutdown(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for AppServerConnection {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Deserialize)]
struct ResponseId {
    id: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct AppServerResponse {
    id: u64,
    #[serde(default, deserialize_with = "deserialize_error_presence")]
    error: bool,
    result: Option<AppServerResult>,
}

#[derive(Debug, Deserialize)]
struct AppServerResult {
    #[serde(rename = "codexHome")]
    codex_home: Option<PathBuf>,
    data: Option<Vec<WireThread>>,
    #[serde(rename = "nextCursor")]
    next_cursor: Option<String>,
}

fn deserialize_error_presence<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde::de::IgnoredAny::deserialize(deserializer).map(|_| true)
}

fn route_app_server_line(line: &str) -> Option<AppServerResponse> {
    serde_json::from_str::<ResponseId>(line).ok()?.id?;
    serde_json::from_str(line).ok()
}

fn locate_codex() -> Option<PathBuf> {
    locate_codex_in_path(std::env::var_os("PATH").as_deref()).or_else(|| {
        #[cfg(target_os = "macos")]
        {
            let bundled = PathBuf::from("/Applications/Codex.app/Contents/Resources/codex");
            is_executable(&bundled).then_some(bundled)
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    })
}

fn locate_codex_in_path(path: Option<&OsStr>) -> Option<PathBuf> {
    std::env::split_paths(path?).find_map(|directory| {
        let candidate = directory.join("codex");
        is_executable(&candidate).then_some(candidate)
    })
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[derive(Debug)]
struct ThreadListPage {
    threads: Vec<CodexThreadMetadata>,
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireThread {
    id: String,
    name: Option<String>,
    cwd: PathBuf,
    path: Option<PathBuf>,
    #[serde(rename = "updatedAt")]
    updated_at: u64,
    status: AppServerStatus,
}

fn parse_thread_list_response(
    payload: &str,
    observed_at: Instant,
) -> Result<ThreadListPage, &'static str> {
    let response = route_app_server_line(payload).ok_or("codex_metadata_invalid_response")?;
    let result = response.result.ok_or("codex_metadata_invalid_response")?;
    parse_thread_list_result(result, observed_at)
}

fn parse_thread_list_result(
    result: AppServerResult,
    observed_at: Instant,
) -> Result<ThreadListPage, &'static str> {
    let data = result.data.ok_or("codex_metadata_invalid_response")?;
    Ok(ThreadListPage {
        threads: data
            .into_iter()
            .map(|thread| CodexThreadMetadata {
                thread_id: thread.id,
                name: thread.name,
                cwd: thread.cwd,
                rollout_path: thread.path,
                server_updated_at: thread.updated_at,
                updated_at: observed_at,
                status: thread.status,
            })
            .collect(),
        next_cursor: result.next_cursor,
    })
}

fn thread_list_params(cursor: Option<&str>) -> Value {
    let mut params = json!({
        "archived": false,
        "limit": 100,
        "sortKey": "updated_at",
        "sortDirection": "desc",
        "useStateDbOnly": true
    });
    if let Some(cursor) = cursor {
        params["cursor"] = Value::String(cursor.to_owned());
    }
    params
}

#[derive(Default)]
struct MetadataTimeAnchors {
    anchors: BTreeMap<String, (u64, Instant)>,
}

impl MetadataTimeAnchors {
    fn resolve(&mut self, thread_id: &str, server_updated_at: u64, now: Instant) -> Instant {
        match self.anchors.get(thread_id) {
            Some((previous, instant)) if *previous == server_updated_at => *instant,
            _ => {
                self.anchors
                    .insert(thread_id.to_owned(), (server_updated_at, now));
                now
            }
        }
    }
}

fn merge_codex_sources(
    now: Instant,
    metadata: Vec<CodexThreadMetadata>,
    rollout: Vec<CodexTaskSnapshot>,
    metadata_health: ChannelHealth,
    rollout_health: ChannelHealth,
) -> CodexSourceSnapshot {
    merge_timed_codex_sources(
        metadata,
        rollout
            .into_iter()
            .map(|task| TimedRolloutTask {
                task,
                updated_at: now,
            })
            .collect(),
        metadata_health,
        rollout_health,
    )
}

struct TimedRolloutTask {
    task: CodexTaskSnapshot,
    updated_at: Instant,
}

fn merge_timed_codex_sources(
    metadata: Vec<CodexThreadMetadata>,
    rollout: Vec<TimedRolloutTask>,
    metadata_health: ChannelHealth,
    rollout_health: ChannelHealth,
) -> CodexSourceSnapshot {
    let mut tasks = BTreeMap::new();
    for thread in metadata {
        let (running, input_need, system_error) = match &thread.status {
            AppServerStatus::NotLoaded | AppServerStatus::Idle => (false, None, false),
            AppServerStatus::SystemError => (false, None, true),
            AppServerStatus::Active { active_flags } => {
                let input_need = if active_flags.contains(&ActiveFlag::WaitingOnApproval) {
                    Some(CodexInputNeed::Approval)
                } else if active_flags.contains(&ActiveFlag::WaitingOnUserInput) {
                    Some(CodexInputNeed::UserInput)
                } else {
                    None
                };
                (true, input_need, false)
            }
        };
        tasks.insert(
            thread.thread_id.clone(),
            MergedCodexTask {
                thread_id: thread.thread_id,
                name: thread.name,
                cwd: thread.cwd,
                updated_at: thread.updated_at,
                running,
                input_need,
                system_error,
                terminal_event: None,
                terminal_sequence: 0,
            },
        );
    }

    for timed_rollout in rollout {
        let rollout_task = timed_rollout.task;
        let task = tasks
            .entry(rollout_task.thread_id.clone())
            .or_insert_with(|| MergedCodexTask {
                thread_id: rollout_task.thread_id.clone(),
                name: None,
                cwd: rollout_task.cwd.clone(),
                updated_at: timed_rollout.updated_at,
                running: false,
                input_need: None,
                system_error: false,
                terminal_event: None,
                terminal_sequence: 0,
            });
        let rollout_contributes_running = rollout_task.running && !task.running;
        task.running |= rollout_task.running;
        if task.input_need.is_none() {
            task.input_need = rollout_task.input_need;
            if rollout_task.input_need.is_some() {
                task.updated_at = task.updated_at.max(timed_rollout.updated_at);
            }
        } else if rollout_contributes_running && !task.system_error {
            task.updated_at = task.updated_at.max(timed_rollout.updated_at);
        }
        if rollout_task.terminal_sequence > task.terminal_sequence {
            task.updated_at = timed_rollout.updated_at;
            task.terminal_event = rollout_task.event;
            task.terminal_sequence = rollout_task.terminal_sequence;
        }
    }

    let health =
        if metadata_health == ChannelHealth::Healthy && rollout_health == ChannelHealth::Healthy {
            SourceHealth::Healthy
        } else {
            SourceHealth::Degraded
        };
    CodexSourceSnapshot {
        health,
        tasks: tasks.into_values().collect(),
    }
}

pub struct CodexTaskSource {
    metadata: Box<dyn CodexMetadataClient>,
    cursor_store_path: PathBuf,
    codex_home: PathBuf,
    sessions_path: PathBuf,
    watcher: Option<RecommendedWatcher>,
    notify_rx: Receiver<notify::Result<Event>>,
    files: BTreeMap<PathBuf, RolloutFileState>,
    metadata_tasks: BTreeMap<String, CodexThreadMetadata>,
    metadata_anchors: MetadataTimeAnchors,
    metadata_health: ChannelHealth,
    rollout_health: ChannelHealth,
    last_metadata_poll: Option<Instant>,
    last_filesystem_poll: Option<Instant>,
    last_publish: Option<Instant>,
    last_seen_server_update: Option<u64>,
    cached_snapshot: Option<CodexSourceSnapshot>,
}

struct RolloutFileState {
    identity: FileIdentity,
    modified: Option<SystemTime>,
    confirmed: FileWindowFingerprint,
    offset: u64,
    trailing: Vec<u8>,
    index: CodexRolloutIndex,
    updated_at: Instant,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileWindowFingerprint {
    start: u64,
    length: u64,
    hash: u64,
}

#[derive(Deserialize, Serialize)]
struct CursorStore {
    version: u32,
    files: Vec<PersistedCursor>,
}

#[derive(Deserialize, Serialize)]
struct PersistedCursor {
    canonical_path: PathBuf,
    identity: FileIdentity,
    byte_offset: u64,
    thread_id: String,
    cwd: PathBuf,
    open_turn_ids: BTreeSet<String>,
    open_call_ids: BTreeSet<String>,
}

impl CodexTaskSource {
    pub fn new(
        metadata: Box<dyn CodexMetadataClient>,
        app_home_fallback: impl AsRef<Path>,
        cursor_store_path: impl AsRef<Path>,
    ) -> Result<Self, &'static str> {
        let app_home_fallback = app_home_fallback.as_ref().to_path_buf();
        let codex_home = resolved_fallback_codex_home(&app_home_fallback);
        let sessions_path = codex_home.join("sessions");
        let (notify_tx, notify_rx) = mpsc::channel();
        let watcher = notify::recommended_watcher(move |event| {
            let _ = notify_tx.send(event);
        })
        .ok();
        let mut source = Self {
            metadata,
            cursor_store_path: cursor_store_path.as_ref().to_path_buf(),
            codex_home,
            sessions_path,
            watcher,
            notify_rx,
            files: BTreeMap::new(),
            metadata_tasks: BTreeMap::new(),
            metadata_anchors: MetadataTimeAnchors::default(),
            metadata_health: ChannelHealth::Unavailable,
            rollout_health: ChannelHealth::Unavailable,
            last_metadata_poll: None,
            last_filesystem_poll: None,
            last_publish: None,
            last_seen_server_update: None,
            cached_snapshot: None,
        };
        source.configure_rollout_home(Instant::now());
        Ok(source)
    }

    fn configure_rollout_home(&mut self, now: Instant) {
        if let Some(watcher) = &mut self.watcher {
            let _ = watcher.unwatch(&self.sessions_path);
        }
        self.files.clear();
        self.sessions_path = self.codex_home.join("sessions");
        if !self.sessions_path.is_dir() {
            self.rollout_health = ChannelHealth::Unavailable;
            return;
        }
        let _watch_active = match &mut self.watcher {
            Some(watcher) => {
                if watcher
                    .watch(&self.sessions_path, RecursiveMode::Recursive)
                    .is_ok()
                {
                    ChannelHealth::Healthy
                } else {
                    ChannelHealth::Unavailable
                }
            }
            None => ChannelHealth::Unavailable,
        };
        let persisted = self.read_cursor_store();
        if self.discover_rollouts(now, &persisted).is_err() {
            self.rollout_health = ChannelHealth::Unavailable;
        } else {
            self.rollout_health = ChannelHealth::Healthy;
        }
        let _ = self.persist_cursors();
    }

    fn read_cursor_store(&self) -> BTreeMap<PathBuf, PersistedCursor> {
        let Ok(payload) = fs::read(&self.cursor_store_path) else {
            return BTreeMap::new();
        };
        let Ok(store) = serde_json::from_slice::<CursorStore>(&payload) else {
            return BTreeMap::new();
        };
        if store.version != CURSOR_VERSION {
            return BTreeMap::new();
        }
        store
            .files
            .into_iter()
            .map(|cursor| (cursor.canonical_path.clone(), cursor))
            .collect()
    }

    fn discover_rollouts(
        &mut self,
        now: Instant,
        persisted: &BTreeMap<PathBuf, PersistedCursor>,
    ) -> Result<(), &'static str> {
        let mut pending = vec![self.sessions_path.clone()];
        while let Some(directory) = pending.pop() {
            let entries = fs::read_dir(directory).map_err(|_| "codex_rollout_scan_failed")?;
            for entry in entries {
                let entry = entry.map_err(|_| "codex_rollout_scan_failed")?;
                let path = entry.path();
                let metadata = entry.metadata().map_err(|_| "codex_rollout_scan_failed")?;
                if metadata.is_dir() {
                    pending.push(path);
                } else if is_recent_rollout(&path, &metadata) {
                    let canonical = path
                        .canonicalize()
                        .map_err(|_| "codex_rollout_scan_failed")?;
                    if let std::collections::btree_map::Entry::Vacant(entry) =
                        self.files.entry(canonical.clone())
                    {
                        let state =
                            recover_rollout_file(&canonical, now, persisted.get(&canonical))?;
                        entry.insert(state);
                    }
                }
            }
        }
        Ok(())
    }

    fn consume_notify_events(&mut self, now: Instant) {
        let mut paths = BTreeSet::new();
        while let Ok(event) = self.notify_rx.try_recv() {
            if let Ok(event) = event {
                paths.extend(event.paths);
            }
        }
        for path in paths {
            if let Ok(canonical) = path.canonicalize() {
                let _ = self.sync_file(&canonical, now);
            }
        }
    }

    fn poll_filesystem(&mut self, now: Instant) {
        self.consume_notify_events(now);
        if self
            .last_filesystem_poll
            .is_some_and(|last| now.saturating_duration_since(last) < FILESYSTEM_POLL_INTERVAL)
        {
            return;
        }
        self.last_filesystem_poll = Some(now);
        let sessions_available = self.sessions_path.is_dir();
        let paths = self.files.keys().cloned().collect::<Vec<_>>();
        let mut readable_file = false;
        for path in paths {
            readable_file |= self.sync_file(&path, now).is_ok();
        }
        readable_file &= !self.files.is_empty();
        self.rollout_health = if sessions_available || readable_file {
            ChannelHealth::Healthy
        } else {
            ChannelHealth::Unavailable
        };
        let _ = self.persist_cursors();
    }

    fn sync_file(&mut self, path: &Path, now: Instant) -> Result<(), &'static str> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(_) => {
                self.files.remove(path);
                return Ok(());
            }
        };
        let identity = file_identity(&metadata);
        let observed_session = first_session_identity(path)?;
        let must_recover = self.files.get(path).is_none_or(|state| {
            let physical_offset = state.offset + state.trailing.len() as u64;
            let session_changed = observed_session.as_ref().is_some_and(|(thread_id, cwd)| {
                state
                    .index
                    .cursor_state()
                    .is_some_and(|cursor| cursor.thread_id != *thread_id || cursor.cwd != *cwd)
            });
            state.identity != identity
                || metadata.len() < physical_offset
                || !file_window_matches(path, state.confirmed)
                || (metadata.len() == physical_offset && metadata.modified().ok() != state.modified)
                || session_changed
        });
        if must_recover {
            let recovered = recover_rollout_file(path, now, None)?;
            self.files.insert(path.to_path_buf(), recovered);
            return Ok(());
        }

        let state = self.files.get_mut(path).expect("rollout state exists");
        let physical_offset = state.offset + state.trailing.len() as u64;
        if metadata.len() == physical_offset {
            return Ok(());
        }
        state.modified = metadata.modified().ok();
        let before = state.index.current_tasks();
        let mut file = File::open(path).map_err(|_| "codex_rollout_read_failed")?;
        file.seek(SeekFrom::Start(physical_offset))
            .map_err(|_| "codex_rollout_read_failed")?;
        file.read_to_end(&mut state.trailing)
            .map_err(|_| "codex_rollout_read_failed")?;
        let Some(last_newline) = state.trailing.iter().rposition(|byte| *byte == b'\n') else {
            return Ok(());
        };
        let complete = state.trailing.drain(..=last_newline).collect::<Vec<_>>();
        state.offset += complete.len() as u64;
        for line in complete.split(|byte| *byte == b'\n') {
            if line.is_empty() {
                continue;
            }
            if let Ok(line) = std::str::from_utf8(line) {
                let _ = state.index.apply_line(line);
            }
        }
        if state.index.current_tasks() != before {
            state.updated_at = now;
        }
        state.confirmed =
            file_window_fingerprint(path, state.offset + state.trailing.len() as u64)?;
        Ok(())
    }

    fn poll_metadata(&mut self, now: Instant) {
        if self
            .last_metadata_poll
            .is_some_and(|last| now.saturating_duration_since(last) < METADATA_POLL_INTERVAL)
        {
            return;
        }
        self.last_metadata_poll = Some(now);
        match self.metadata.poll_updated(self.last_seen_server_update) {
            Ok(mut threads) => {
                self.metadata_health = ChannelHealth::Healthy;
                let reported_home = self.metadata.codex_home().to_path_buf();
                if reported_home != self.codex_home && reported_home.is_dir() {
                    self.codex_home = reported_home;
                    self.configure_rollout_home(now);
                }
                for thread in &mut threads {
                    thread.updated_at = self.metadata_anchors.resolve(
                        &thread.thread_id,
                        thread.server_updated_at,
                        now,
                    );
                    self.last_seen_server_update = Some(
                        self.last_seen_server_update
                            .unwrap_or_default()
                            .max(thread.server_updated_at),
                    );
                }
                for path in threads
                    .iter()
                    .filter_map(|thread| thread.rollout_path.as_deref())
                {
                    let _ = self.track_app_server_rollout(path, now);
                }
                self.metadata_tasks.extend(
                    threads
                        .into_iter()
                        .map(|thread| (thread.thread_id.clone(), thread)),
                );
            }
            Err(_) => self.metadata_health = ChannelHealth::Unavailable,
        }
    }

    fn build_snapshot(&self) -> CodexSourceSnapshot {
        let mut rollout = Vec::new();
        for state in self.files.values() {
            for task in state.index.current_tasks() {
                rollout.push(TimedRolloutTask {
                    task,
                    updated_at: state.updated_at,
                });
            }
        }
        merge_timed_codex_sources(
            self.metadata_tasks.values().cloned().collect(),
            rollout,
            self.metadata_health,
            self.rollout_health,
        )
    }

    fn track_app_server_rollout(&mut self, path: &Path, now: Instant) -> Result<(), &'static str> {
        let canonical = path
            .canonicalize()
            .map_err(|_| "codex_rollout_read_failed")?;
        if let std::collections::btree_map::Entry::Vacant(entry) =
            self.files.entry(canonical.clone())
        {
            let state = recover_rollout_file(&canonical, now, None)?;
            entry.insert(state);
        }
        if let Some(watcher) = &mut self.watcher {
            let _ = watcher.watch(&canonical, RecursiveMode::NonRecursive);
        }
        self.rollout_health = ChannelHealth::Healthy;
        Ok(())
    }

    fn persist_cursors(&self) -> Result<(), &'static str> {
        let files = self
            .files
            .iter()
            .filter_map(|(path, state)| {
                let cursor = state.index.cursor_state()?;
                Some(PersistedCursor {
                    canonical_path: path.clone(),
                    identity: state.identity,
                    byte_offset: state.offset,
                    thread_id: cursor.thread_id,
                    cwd: cursor.cwd,
                    open_turn_ids: cursor.open_turn_ids,
                    open_call_ids: cursor.open_call_ids,
                })
            })
            .collect();
        let store = CursorStore {
            version: CURSOR_VERSION,
            files,
        };
        let payload = serde_json::to_vec(&store).map_err(|_| "codex_cursor_serialize_failed")?;
        let parent = self
            .cursor_store_path
            .parent()
            .ok_or("codex_cursor_path_invalid")?;
        fs::create_dir_all(parent).map_err(|_| "codex_cursor_write_failed")?;
        let temporary = self.cursor_store_path.with_extension("json.tmp");
        fs::write(&temporary, payload).map_err(|_| "codex_cursor_write_failed")?;
        fs::rename(&temporary, &self.cursor_store_path).map_err(|_| "codex_cursor_write_failed")
    }
}

impl CodexTaskReader for CodexTaskSource {
    fn poll_tasks(&mut self, now: Instant) -> Result<CodexSourceSnapshot, &'static str> {
        if self
            .last_publish
            .is_some_and(|last| now.saturating_duration_since(last) < MIN_PUBLISH_INTERVAL)
            && let Some(snapshot) = &self.cached_snapshot
        {
            return Ok(snapshot.clone());
        }
        self.poll_metadata(now);
        self.poll_filesystem(now);
        if self.metadata_health == ChannelHealth::Unavailable
            && self.rollout_health == ChannelHealth::Unavailable
        {
            return Err("codex_channels_unavailable");
        }
        let snapshot = self.build_snapshot();
        self.last_publish = Some(now);
        self.cached_snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }
}

fn resolved_fallback_codex_home(app_home_fallback: &Path) -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| app_home_fallback.join(".codex"))
}

fn is_recent_rollout(path: &Path, metadata: &fs::Metadata) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "jsonl")
        && metadata
            .modified()
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age <= RECENT_ROLLOUT_AGE)
}

#[cfg(unix)]
fn file_identity(metadata: &fs::Metadata) -> FileIdentity {
    use std::os::unix::fs::MetadataExt;
    FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}

#[cfg(not(unix))]
fn file_identity(metadata: &fs::Metadata) -> FileIdentity {
    FileIdentity {
        device: 0,
        inode: metadata.len(),
    }
}

fn recover_rollout_file(
    path: &Path,
    now: Instant,
    persisted: Option<&PersistedCursor>,
) -> Result<RolloutFileState, &'static str> {
    let metadata = fs::metadata(path).map_err(|_| "codex_rollout_read_failed")?;
    let identity = file_identity(&metadata);
    if let Some(persisted) = persisted
        && persisted.identity == identity
        && metadata.len() >= persisted.byte_offset
    {
        let mut index = CodexRolloutIndex::default();
        index.restore_cursor_state(CodexRolloutCursorState {
            thread_id: persisted.thread_id.clone(),
            cwd: persisted.cwd.clone(),
            open_turn_ids: persisted.open_turn_ids.clone(),
            open_call_ids: persisted.open_call_ids.clone(),
        });
        let mut state = RolloutFileState {
            identity,
            modified: metadata.modified().ok(),
            confirmed: file_window_fingerprint(path, persisted.byte_offset)?,
            offset: persisted.byte_offset,
            trailing: Vec::new(),
            index,
            updated_at: file_time_as_instant(&metadata, now),
        };
        apply_initial_tail(path, &mut state)?;
        state.confirmed = file_window_fingerprint(path, state.offset)?;
        return Ok(state);
    }

    let complete_end = last_complete_offset(path, metadata.len())?;
    let boundary = latest_turn_boundary_offset(path, complete_end)?;
    let mut index = CodexRolloutIndex::default();
    if let Some(session_line) = first_complete_line(path)? {
        index
            .apply_initial_scan(std::iter::once(session_line.as_str()))
            .map_err(|_| "codex_rollout_parse_failed")?;
    }
    if let Some(boundary) = boundary {
        apply_initial_range(path, boundary, complete_end, &mut index)?;
    }
    Ok(RolloutFileState {
        identity,
        modified: metadata.modified().ok(),
        confirmed: file_window_fingerprint(path, complete_end)?,
        offset: complete_end,
        trailing: Vec::new(),
        index,
        updated_at: file_time_as_instant(&metadata, now),
    })
}

fn apply_initial_tail(path: &Path, state: &mut RolloutFileState) -> Result<(), &'static str> {
    let metadata = fs::metadata(path).map_err(|_| "codex_rollout_read_failed")?;
    let complete_end = last_complete_offset(path, metadata.len())?;
    if complete_end > state.offset {
        apply_initial_range(path, state.offset, complete_end, &mut state.index)?;
        state.offset = complete_end;
    }
    Ok(())
}

fn apply_initial_range(
    path: &Path,
    start: u64,
    end: u64,
    index: &mut CodexRolloutIndex,
) -> Result<(), &'static str> {
    let mut file = File::open(path).map_err(|_| "codex_rollout_read_failed")?;
    file.seek(SeekFrom::Start(start))
        .map_err(|_| "codex_rollout_read_failed")?;
    let reader = BufReader::new(file.take(end.saturating_sub(start)));
    for line in reader.lines() {
        let line = line.map_err(|_| "codex_rollout_read_failed")?;
        index
            .apply_initial_scan(std::iter::once(line.as_str()))
            .map_err(|_| "codex_rollout_parse_failed")?;
    }
    Ok(())
}

fn first_complete_line(path: &Path) -> Result<Option<String>, &'static str> {
    let file = File::open(path).map_err(|_| "codex_rollout_read_failed")?;
    let mut line = String::new();
    BufReader::new(file)
        .take(RECOVERY_CHUNK_SIZE as u64)
        .read_line(&mut line)
        .map_err(|_| "codex_rollout_read_failed")?;
    if !line.ends_with('\n') {
        return Ok(None);
    }
    line.pop();
    Ok(Some(line))
}

fn first_session_identity(path: &Path) -> Result<Option<(String, PathBuf)>, &'static str> {
    let Some(line) = first_complete_line(path)? else {
        return Ok(None);
    };
    match parse_rollout_line(&line) {
        Ok(Some(CodexRolloutEvent::Session { thread_id, cwd })) => Ok(Some((thread_id, cwd))),
        Ok(_) => Ok(None),
        Err(_) => Err("codex_rollout_parse_failed"),
    }
}

fn last_complete_offset(path: &Path, length: u64) -> Result<u64, &'static str> {
    let mut file = File::open(path).map_err(|_| "codex_rollout_read_failed")?;
    let mut end = length;
    while end > 0 {
        let start = end.saturating_sub(RECOVERY_CHUNK_SIZE as u64);
        file.seek(SeekFrom::Start(start))
            .map_err(|_| "codex_rollout_read_failed")?;
        let mut chunk = vec![0; (end - start) as usize];
        file.read_exact(&mut chunk)
            .map_err(|_| "codex_rollout_read_failed")?;
        if let Some(position) = chunk.iter().rposition(|byte| *byte == b'\n') {
            return Ok(start + position as u64 + 1);
        }
        end = start;
    }
    Ok(0)
}

fn latest_turn_boundary_offset(
    path: &Path,
    complete_end: u64,
) -> Result<Option<u64>, &'static str> {
    let mut end = complete_end;
    let mut newer_prefix = Vec::new();
    while end > 0 {
        let start = end.saturating_sub(RECOVERY_CHUNK_SIZE as u64);
        let mut file = File::open(path).map_err(|_| "codex_rollout_read_failed")?;
        file.seek(SeekFrom::Start(start))
            .map_err(|_| "codex_rollout_read_failed")?;
        let mut chunk = vec![0; (end - start) as usize];
        file.read_exact(&mut chunk)
            .map_err(|_| "codex_rollout_read_failed")?;
        chunk.extend_from_slice(&newer_prefix);
        let mut line_start = 0;
        let mut latest = None;
        for (position, byte) in chunk.iter().enumerate() {
            if *byte != b'\n' {
                continue;
            }
            if let Ok(line) = std::str::from_utf8(&chunk[line_start..position])
                && is_turn_boundary(line)
                && line_start < (end - start) as usize
            {
                latest = Some(start + line_start as u64);
            }
            line_start = position + 1;
        }
        if latest.is_some() {
            return Ok(latest);
        }
        let prefix_end = chunk
            .iter()
            .position(|byte| *byte == b'\n')
            .unwrap_or(chunk.len());
        newer_prefix = chunk[..prefix_end].to_vec();
        end = start;
    }
    Ok(None)
}

fn is_turn_boundary(line: &str) -> bool {
    matches!(
        parse_rollout_line(line),
        Ok(Some(
            CodexRolloutEvent::TurnStarted { .. }
                | CodexRolloutEvent::TurnCompleted { .. }
                | CodexRolloutEvent::TurnInterrupted { .. }
        ))
    )
}

fn file_time_as_instant(metadata: &fs::Metadata, now: Instant) -> Instant {
    metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .and_then(|age| now.checked_sub(age))
        .unwrap_or(now)
}

fn file_window_fingerprint(
    path: &Path,
    confirmed_end: u64,
) -> Result<FileWindowFingerprint, &'static str> {
    let start = confirmed_end.saturating_sub(CONFIRMED_WINDOW_SIZE);
    let length = confirmed_end - start;
    let mut file = File::open(path).map_err(|_| "codex_rollout_read_failed")?;
    file.seek(SeekFrom::Start(start))
        .map_err(|_| "codex_rollout_read_failed")?;
    let mut bytes = vec![0; length as usize];
    file.read_exact(&mut bytes)
        .map_err(|_| "codex_rollout_read_failed")?;
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    Ok(FileWindowFingerprint {
        start,
        length,
        hash: hasher.finish(),
    })
}

fn file_window_matches(path: &Path, expected: FileWindowFingerprint) -> bool {
    file_window_fingerprint(path, expected.start + expected.length)
        .is_ok_and(|actual| actual == expected)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeSet, VecDeque},
        fs::{self, OpenOptions},
        io::Write,
        path::PathBuf,
        time::{Duration, Instant},
    };

    use super::*;
    use crate::display::{
        CodexInputNeed, CodexTaskSnapshot, DisplayProvider, SourceHealth,
        codex_provider::CodexDisplayProvider,
    };
    use tempfile::TempDir;

    struct FakeMetadataClient {
        codex_home: PathBuf,
        polls: VecDeque<Result<Vec<CodexThreadMetadata>, String>>,
    }

    impl FakeMetadataClient {
        fn healthy(codex_home: PathBuf) -> Self {
            Self {
                codex_home,
                polls: VecDeque::from([Ok(Vec::new())]),
            }
        }

        fn unavailable(codex_home: PathBuf) -> Self {
            Self {
                codex_home,
                polls: VecDeque::from([Err("metadata_unavailable".into())]),
            }
        }
    }

    impl CodexMetadataClient for FakeMetadataClient {
        fn codex_home(&self) -> &std::path::Path {
            &self.codex_home
        }

        fn poll_updated(
            &mut self,
            _last_seen: Option<u64>,
        ) -> Result<Vec<CodexThreadMetadata>, String> {
            self.polls.pop_front().unwrap_or_else(|| Ok(Vec::new()))
        }
    }

    fn rollout_path(temp: &TempDir) -> PathBuf {
        let sessions = temp.path().join(".codex/sessions/2026/08/10");
        fs::create_dir_all(&sessions).unwrap();
        sessions.join("rollout-test.jsonl")
    }

    fn source_for(temp: &TempDir, metadata: FakeMetadataClient) -> CodexTaskSource {
        CodexTaskSource::new(
            Box::new(metadata),
            temp.path(),
            temp.path().join("app-data/display/codex-cursors-v1.json"),
        )
        .unwrap()
    }

    fn thread(
        now: Instant,
        thread_id: &str,
        cwd: &str,
        status: AppServerStatus,
    ) -> CodexThreadMetadata {
        CodexThreadMetadata {
            thread_id: thread_id.into(),
            name: None,
            cwd: PathBuf::from(cwd),
            rollout_path: None,
            server_updated_at: 10,
            updated_at: now,
            status,
        }
    }

    fn task(thread_id: &str, cwd: &str, running: bool, input: bool) -> CodexTaskSnapshot {
        CodexTaskSnapshot {
            thread_id: thread_id.into(),
            cwd: PathBuf::from(cwd),
            running,
            input_need: input.then_some(CodexInputNeed::UserInput),
            terminal_sequence: 0,
            event: None,
        }
    }

    #[test]
    fn explicit_active_flags_override_not_loaded_but_rollout_running_survives_it() {
        let now = Instant::now();
        let metadata = vec![
            thread(
                now,
                "thread-a",
                "/work/kivo",
                AppServerStatus::Active {
                    active_flags: BTreeSet::from([ActiveFlag::WaitingOnUserInput]),
                },
            ),
            thread(
                now,
                "thread-b",
                "/work/mindcraft",
                AppServerStatus::NotLoaded,
            ),
        ];
        let rollout = vec![task("thread-b", "/work/mindcraft", true, false)];

        let snapshot = merge_codex_sources(
            now,
            metadata,
            rollout,
            ChannelHealth::Healthy,
            ChannelHealth::Healthy,
        );

        assert_eq!(
            snapshot.task("thread-a").unwrap().input_need,
            Some(CodexInputNeed::UserInput)
        );
        assert!(snapshot.task("thread-b").unwrap().running);
    }

    #[test]
    fn approval_comes_only_from_the_explicit_app_server_flag() {
        let now = Instant::now();
        let approval = thread(
            now,
            "thread-a",
            "/work/kivo",
            AppServerStatus::Active {
                active_flags: BTreeSet::from([ActiveFlag::WaitingOnApproval]),
            },
        );
        let rollout_input = task("thread-b", "/work/mindcraft", true, true);

        let snapshot = merge_codex_sources(
            now,
            vec![approval],
            vec![rollout_input],
            ChannelHealth::Healthy,
            ChannelHealth::Healthy,
        );

        assert_eq!(
            snapshot.task("thread-a").unwrap().input_need,
            Some(CodexInputNeed::Approval)
        );
        assert_eq!(
            snapshot.task("thread-b").unwrap().input_need,
            Some(CodexInputNeed::UserInput)
        );
    }

    #[test]
    fn one_healthy_channel_keeps_the_combined_source_degraded_not_offline() {
        let snapshot = merge_codex_sources(
            Instant::now(),
            vec![],
            vec![],
            ChannelHealth::Unavailable,
            ChannelHealth::Healthy,
        );
        assert_eq!(snapshot.health, SourceHealth::Degraded);
    }

    #[test]
    fn request_payload_is_read_only_and_incremental() {
        assert_eq!(
            thread_list_params(None),
            serde_json::json!({
                "archived": false,
                "limit": 100,
                "sortKey": "updated_at",
                "sortDirection": "desc",
                "useStateDbOnly": true
            })
        );
        assert_eq!(
            thread_list_params(Some("next-page")),
            serde_json::json!({
                "archived": false,
                "cursor": "next-page",
                "limit": 100,
                "sortKey": "updated_at",
                "sortDirection": "desc",
                "useStateDbOnly": true
            })
        );
    }

    #[test]
    fn generated_thread_list_subset_deserializes_without_conversation_fields() {
        let now = Instant::now();
        let response = parse_thread_list_response(
            include_str!("../../tests/fixtures/codex/thread-list-response.json"),
            now,
        )
        .unwrap();

        assert_eq!(response.threads.len(), 2);
        assert_eq!(response.threads[0].thread_id, "thread-a");
        assert_eq!(response.threads[0].name.as_deref(), Some("OLED design"));
        assert_eq!(response.threads[0].updated_at, now);
        assert_eq!(response.next_cursor, None);
        assert!(!format!("{response:?}").contains("preview"));
    }

    #[test]
    fn stable_server_timestamp_keeps_metadata_event_time_stable() {
        let now = Instant::now();
        let mut anchors = MetadataTimeAnchors::default();
        assert_eq!(anchors.resolve("thread-a", 10, now), now);
        assert_eq!(
            anchors.resolve("thread-a", 10, now + Duration::from_secs(3)),
            now
        );
        assert_eq!(
            anchors.resolve("thread-a", 11, now + Duration::from_secs(3)),
            now + Duration::from_secs(3)
        );
    }

    #[test]
    fn startup_recovery_restores_open_state_without_terminal_alerts() {
        let temp = TempDir::new().unwrap();
        let path = rollout_path(&temp);
        fs::write(
            &path,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-a\",\"cwd\":\"/work/kivo\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"turn-a\"}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"content\":\"must not persist\"}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"request_user_input\",\"call_id\":\"call-a\"}}\n",
            ),
        )
        .unwrap();
        let metadata = FakeMetadataClient::healthy(temp.path().join(".codex"));
        let mut source = source_for(&temp, metadata);

        let snapshot = source.poll_tasks(Instant::now()).unwrap();
        let task = snapshot.task("thread-a").unwrap();
        assert!(task.running);
        assert_eq!(task.input_need, Some(CodexInputNeed::UserInput));
        assert_eq!(task.terminal_event, None);
        assert_eq!(task.terminal_sequence, 0);

        let cursor =
            fs::read_to_string(temp.path().join("app-data/display/codex-cursors-v1.json")).unwrap();
        assert!(!cursor.contains("must not persist"));
        assert!(!cursor.contains("content"));
        assert!(cursor.contains("thread-a"));
        assert!(cursor.contains("turn-a"));
        assert!(cursor.contains("call-a"));
    }

    #[test]
    fn trailing_incomplete_record_is_applied_only_after_newline_termination() {
        let temp = TempDir::new().unwrap();
        let path = rollout_path(&temp);
        fs::write(
            &path,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-a\",\"cwd\":\"/work/kivo\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"turn-a\"}}\n",
            ),
        )
        .unwrap();
        let metadata = FakeMetadataClient::healthy(temp.path().join(".codex"));
        let mut source = source_for(&temp, metadata);
        let now = Instant::now();
        source.poll_tasks(now).unwrap();

        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",")
            .unwrap();
        file.flush().unwrap();
        let partial = source.poll_tasks(now + Duration::from_secs(1)).unwrap();
        assert_eq!(partial.task("thread-a").unwrap().input_need, None);

        file.write_all(b"\"name\":\"request_user_input\",\"call_id\":\"call-a\"}}\n")
            .unwrap();
        file.flush().unwrap();
        let complete = source.poll_tasks(now + Duration::from_secs(2)).unwrap();
        assert_eq!(
            complete.task("thread-a").unwrap().input_need,
            Some(CodexInputNeed::UserInput)
        );
    }

    #[test]
    fn truncation_rebuilds_file_state_with_initial_alert_suppression() {
        let temp = TempDir::new().unwrap();
        let path = rollout_path(&temp);
        fs::write(
            &path,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-a\",\"cwd\":\"/work/kivo\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"turn-a\"}}\n",
            ),
        )
        .unwrap();
        let metadata = FakeMetadataClient::healthy(temp.path().join(".codex"));
        let mut source = source_for(&temp, metadata);
        let now = Instant::now();
        source.poll_tasks(now).unwrap();

        fs::write(
            &path,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-b\",\"cwd\":\"/work/mindcraft\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"turn-b\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"turn-b\"}}\n",
            ),
        )
        .unwrap();
        let snapshot = source.poll_tasks(now + Duration::from_secs(1)).unwrap();
        assert!(snapshot.task("thread-a").is_none());
        let task = snapshot.task("thread-b").unwrap();
        assert!(!task.running);
        assert_eq!(task.terminal_event, None);
    }

    #[test]
    fn both_unavailable_channels_return_the_static_source_error() {
        let temp = TempDir::new().unwrap();
        let metadata = FakeMetadataClient::unavailable(temp.path().join("missing-codex"));
        let mut source = source_for(&temp, metadata);

        assert_eq!(
            source.poll_tasks(Instant::now()).unwrap_err(),
            "codex_channels_unavailable"
        );
    }

    #[test]
    fn response_router_ignores_notifications_and_non_numeric_ids() {
        assert!(
            route_app_server_line(r#"{"method":"thread/status/changed","params":{}}"#).is_none()
        );
        assert!(route_app_server_line(r#"{"id":"two","result":{}}"#).is_none());
        let response = route_app_server_line(r#"{"id":2,"result":{"data":[]}}"#).unwrap();
        assert_eq!(response.id, 2);
        assert!(response.result.unwrap().data.unwrap().is_empty());

        let routed = route_app_server_line(
            r#"{"id":3,"result":{"data":[{"id":"thread-a","name":null,"cwd":"/work/kivo","path":null,"updatedAt":10,"status":{"type":"idle"},"preview":"must not retain"}],"nextCursor":null}}"#,
        )
        .unwrap();
        assert!(!format!("{routed:?}").contains("must not retain"));
    }

    #[cfg(unix)]
    #[test]
    fn executable_discovery_uses_path_entries() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let executable = temp.path().join("codex");
        fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(
            locate_codex_in_path(Some(temp.path().as_os_str())),
            Some(executable)
        );
    }

    #[test]
    fn system_metadata_client_is_send_and_starts_disconnected() {
        fn assert_send<T: Send>() {}
        assert_send::<SystemCodexMetadataClient>();
        let temp = TempDir::new().unwrap();
        let client = SystemCodexMetadataClient::new(temp.path().join(".codex"));
        assert_eq!(client.codex_home(), temp.path().join(".codex"));
        assert!(!client.is_connected());
    }

    #[test]
    fn app_server_rollout_path_bypasses_the_fallback_age_bound() {
        let temp = TempDir::new().unwrap();
        let path = rollout_path(&temp);
        fs::write(
            &path,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-old\",\"cwd\":\"/work/kivo\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"turn-old\"}}\n",
            ),
        )
        .unwrap();
        let old = SystemTime::now() - Duration::from_secs(25 * 60 * 60);
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();
        let metadata_thread = CodexThreadMetadata {
            thread_id: "thread-old".into(),
            name: None,
            cwd: PathBuf::from("/work/kivo"),
            rollout_path: Some(path),
            server_updated_at: 10,
            updated_at: Instant::now(),
            status: AppServerStatus::NotLoaded,
        };
        let metadata = FakeMetadataClient {
            codex_home: temp.path().join(".codex"),
            polls: VecDeque::from([Ok(vec![metadata_thread])]),
        };
        let mut source = source_for(&temp, metadata);

        let snapshot = source.poll_tasks(Instant::now()).unwrap();
        assert!(snapshot.task("thread-old").unwrap().running);
    }

    #[test]
    fn repeated_source_and_provider_polls_do_not_extend_terminal_expiry() {
        let temp = TempDir::new().unwrap();
        let path = rollout_path(&temp);
        fs::write(
            &path,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-a\",\"cwd\":\"/work/kivo\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"turn-a\"}}\n",
            ),
        )
        .unwrap();
        let metadata = FakeMetadataClient::healthy(temp.path().join(".codex"));
        let source = source_for(&temp, metadata);
        let mut provider = CodexDisplayProvider::new(source);
        let now = Instant::now();
        provider.poll(now).unwrap();

        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(
            b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"turn-a\"}}\n",
        )
        .unwrap();
        file.flush().unwrap();

        let first = provider.poll(now + Duration::from_secs(1)).unwrap();
        let first_expiry = first
            .items
            .iter()
            .find(|item| item.id == "codex.task.thread-a")
            .unwrap()
            .expires_at;
        let repeated = provider.poll(now + Duration::from_secs(5)).unwrap();
        let repeated_expiry = repeated
            .items
            .iter()
            .find(|item| item.id == "codex.task.thread-a")
            .unwrap()
            .expires_at;

        assert_eq!(first_expiry, Some(now + Duration::from_secs(9)));
        assert_eq!(repeated_expiry, first_expiry);
    }

    #[test]
    fn recovery_reverse_scans_past_a_large_incomplete_tail() {
        let temp = TempDir::new().unwrap();
        let path = rollout_path(&temp);
        let complete = concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-a\",\"cwd\":\"/work/kivo\"}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"turn-a\"}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"turn-a\"}}\n",
        );
        let mut payload = complete.as_bytes().to_vec();
        payload.extend(std::iter::repeat_n(b'x', RECOVERY_CHUNK_SIZE + 4096));
        fs::write(&path, payload).unwrap();

        assert_eq!(
            last_complete_offset(&path, fs::metadata(&path).unwrap().len()).unwrap(),
            complete.len() as u64
        );
        let recovered = recover_rollout_file(&path, Instant::now(), None).unwrap();
        assert_eq!(recovered.offset, complete.len() as u64);
        let task = &recovered.index.current_tasks()[0];
        assert_eq!(task.event, None);
        assert_eq!(task.terminal_sequence, 0);
    }

    #[test]
    fn equal_timestamp_boundary_keeps_new_and_changed_threads_across_pages() {
        let now = Instant::now();
        let baseline = thread(now, "thread-a", "/work/a", AppServerStatus::Idle);
        let mut watermark = MetadataWatermark::default();
        watermark.commit(&[baseline]);

        let changed = thread(
            now,
            "thread-a",
            "/work/a",
            AppServerStatus::Active {
                active_flags: BTreeSet::new(),
            },
        );
        let new_b = thread(now, "thread-b", "/work/b", AppServerStatus::Idle);
        let new_c = thread(now, "thread-c", "/work/c", AppServerStatus::Idle);
        let mut older = thread(now, "thread-old", "/work/old", AppServerStatus::Idle);
        older.server_updated_at = 9;
        let mut seen_ids = BTreeSet::new();

        let (first, reached_older_first) =
            watermark.select_page(Some(10), vec![changed, new_b], &mut seen_ids);
        let (second, reached_older_second) =
            watermark.select_page(Some(10), vec![new_c, older], &mut seen_ids);

        assert!(!reached_older_first);
        assert!(reached_older_second);
        assert_eq!(
            first
                .iter()
                .chain(&second)
                .map(|thread| thread.thread_id.as_str())
                .collect::<Vec<_>>(),
            ["thread-a", "thread-b", "thread-c"]
        );
    }

    #[test]
    fn repeated_pagination_cursor_is_rejected() {
        let mut seen = BTreeSet::new();
        assert_eq!(
            advance_pagination(Some("same".into()), &mut seen, 1).unwrap(),
            Some("same".into())
        );
        assert_eq!(
            advance_pagination(Some("same".into()), &mut seen, 2).unwrap_err(),
            "codex_app_server_pagination_loop"
        );
    }

    #[test]
    fn same_inode_rewrite_regrown_past_old_offset_rebuilds_state() {
        let temp = TempDir::new().unwrap();
        let path = rollout_path(&temp);
        let initial = concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-a\",\"cwd\":\"/work/kivo\"}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"turn-a\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"request_user_input\",\"call_id\":\"call-a\"}}\n",
        );
        fs::write(&path, initial).unwrap();
        let metadata = FakeMetadataClient::healthy(temp.path().join(".codex"));
        let mut source = source_for(&temp, metadata);
        let now = Instant::now();
        assert_eq!(
            source
                .poll_tasks(now)
                .unwrap()
                .task("thread-a")
                .unwrap()
                .input_need,
            Some(CodexInputNeed::UserInput)
        );

        let mut replacement = concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-a\",\"cwd\":\"/work/kivo\"}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"turn-b\"}}\n",
        )
        .to_owned();
        for _ in 0..12 {
            replacement.push_str(
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"content\":\"ignored\"}}\n",
            );
        }
        assert!(replacement.len() > initial.len());
        fs::write(&path, replacement).unwrap();

        let rebuilt = source.poll_tasks(now + Duration::from_secs(1)).unwrap();
        let task = rebuilt.task("thread-a").unwrap();
        assert!(task.running);
        assert_eq!(task.input_need, None);
    }

    #[test]
    fn runtime_stat_poll_updates_known_files_without_recursive_discovery() {
        let temp = TempDir::new().unwrap();
        let known = rollout_path(&temp);
        fs::write(
            &known,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-known\",\"cwd\":\"/work/kivo\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"turn-known\"}}\n",
            ),
        )
        .unwrap();
        let metadata = FakeMetadataClient::healthy(temp.path().join(".codex"));
        let mut source = source_for(&temp, metadata);
        source.watcher.take();
        while source.notify_rx.try_recv().is_ok() {}

        let unknown = known.with_file_name("rollout-unknown.jsonl");
        fs::write(
            &unknown,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-unknown\",\"cwd\":\"/work/other\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"turn-unknown\"}}\n",
            ),
        )
        .unwrap();
        let mut known_file = OpenOptions::new().append(true).open(&known).unwrap();
        known_file
            .write_all(
                b"{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"request_user_input\",\"call_id\":\"call-known\"}}\n",
            )
            .unwrap();
        known_file.flush().unwrap();

        let snapshot = source
            .poll_tasks(Instant::now() + Duration::from_secs(1))
            .unwrap();
        assert_eq!(
            snapshot.task("thread-known").unwrap().input_need,
            Some(CodexInputNeed::UserInput)
        );
        assert!(snapshot.task("thread-unknown").is_none());
    }
}
