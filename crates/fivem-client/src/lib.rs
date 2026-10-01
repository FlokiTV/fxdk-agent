use std::{
    error::Error,
    ffi::OsString,
    fmt, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use fxdk_agent_config::AppConfig;
use fxdk_agent_process_supervisor::{
    ProcessPhase, ProcessSpec, ProcessSupervisor,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

pub const DEFAULT_CLIENT_ID: u32 = 1;
pub const DEFAULT_RUNTIME_WEB_BASE_URL: &str = "http://127.0.0.1:35419";
pub const DEFAULT_CONTROL_API_BASE_URL: &str = "http://127.0.0.1:35418";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FivemClientPhase {
    Stopped,
    Starting,
    Connecting,
    Active,
    Stopping,
    Crashed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClientEventKind {
    SdkReady,
    GameLaunched,
    ConnectRequested,
    ProcessState,
    ConnectionState,
    Heartbeat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FivemClientEvent {
    pub client_id: u32,
    pub kind: ClientEventKind,
    #[serde(default)]
    pub current: Option<u32>,
    #[serde(default)]
    pub previous: Option<u32>,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub server_address: Option<String>,
    #[serde(default)]
    pub game_process_state: Option<u32>,
    #[serde(default)]
    pub connection_state: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FivemClientSnapshot {
    pub id: u32,
    pub phase: FivemClientPhase,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub last_error: Option<String>,
    pub connection_state: Option<u32>,
    pub game_process_state: Option<u32>,
    pub log_tail: Vec<String>,
}

#[derive(Debug)]
struct FivemClientState {
    phase: FivemClientPhase,
    pid: Option<u32>,
    exit_code: Option<i32>,
    last_error: Option<String>,
    connection_state: Option<u32>,
    game_process_state: Option<u32>,
}

impl Default for FivemClientState {
    fn default() -> Self {
        Self {
            phase: FivemClientPhase::Stopped,
            pid: None,
            exit_code: None,
            last_error: None,
            connection_state: None,
            game_process_state: None,
        }
    }
}

#[derive(Debug)]
pub enum FivemClientError {
    MissingFiveMPath,
    InvalidFiveMPath(PathBuf),
    InvalidSdkRoot(PathBuf),
    AlreadyRunning,
    WrongClientId { expected: u32, actual: u32 },
    Spawn(io::Error),
    Stop(io::Error),
}

impl fmt::Display for FivemClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFiveMPath => write!(formatter, "FiveM executable is not configured"),
            Self::InvalidFiveMPath(path) => {
                write!(formatter, "FiveM executable is not a file: {}", path.display())
            }
            Self::InvalidSdkRoot(path) => {
                write!(formatter, "FxDK SDK root is invalid: {}", path.display())
            }
            Self::AlreadyRunning => write!(formatter, "FiveM/FxDK client is already running"),
            Self::WrongClientId { expected, actual } => write!(
                formatter,
                "runtime event belongs to client {actual}, expected client {expected}"
            ),
            Self::Spawn(error) => write!(formatter, "failed to spawn FiveM/FxDK: {error}"),
            Self::Stop(error) => write!(formatter, "failed to stop FiveM/FxDK: {error}"),
        }
    }
}

impl Error for FivemClientError {}

#[derive(Clone)]
pub struct FivemClientController {
    id: u32,
    supervisor: ProcessSupervisor,
    state: Arc<RwLock<FivemClientState>>,
    operation: Arc<Mutex<()>>,
}

impl Default for FivemClientController {
    fn default() -> Self {
        Self::new(DEFAULT_CLIENT_ID)
    }
}

impl FivemClientController {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            supervisor: ProcessSupervisor::default(),
            state: Arc::new(RwLock::new(FivemClientState::default())),
            operation: Arc::new(Mutex::new(())),
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub async fn start(
        &self,
        config: &AppConfig,
        sdk_root: &Path,
    ) -> Result<FivemClientSnapshot, FivemClientError> {
        let _operation = self.operation.lock().await;
        self.reconcile().await;

        {
            let state = self.state.read().await;
            if matches!(
                state.phase,
                FivemClientPhase::Starting
                    | FivemClientPhase::Connecting
                    | FivemClientPhase::Active
            ) {
                return Err(FivemClientError::AlreadyRunning);
            }
        }

        let spec = build_fivem_spec(config, sdk_root, self.id)?;

        {
            let mut state = self.state.write().await;
            state.phase = FivemClientPhase::Starting;
            state.pid = None;
            state.exit_code = None;
            state.last_error = None;
            state.connection_state = None;
            state.game_process_state = None;
        }

        let process = match self.supervisor.spawn(spec).await {
            Ok(process) => process,
            Err(error) => {
                let message = error.to_string();
                self.set_crashed(None, message).await;
                return Err(FivemClientError::Spawn(error));
            }
        };

        {
            let mut state = self.state.write().await;
            state.phase = FivemClientPhase::Connecting;
            state.pid = process.pid;
        }

        Ok(self.snapshot().await)
    }

    pub async fn stop(&self) -> Result<FivemClientSnapshot, FivemClientError> {
        let _operation = self.operation.lock().await;
        self.reconcile().await;

        {
            let state = self.state.read().await;
            if state.phase == FivemClientPhase::Stopped {
                drop(state);
                return Ok(self.snapshot().await);
            }
        }

        {
            let mut state = self.state.write().await;
            state.phase = FivemClientPhase::Stopping;
        }

        let process = self
            .supervisor
            .stop()
            .await
            .map_err(FivemClientError::Stop)?;

        {
            let mut state = self.state.write().await;
            state.phase = FivemClientPhase::Stopped;
            state.pid = None;
            state.exit_code = process.exit_code;
            state.last_error = None;
            state.connection_state = None;
            state.game_process_state = None;
        }

        Ok(self.snapshot().await)
    }

    pub async fn record_event(
        &self,
        event: FivemClientEvent,
    ) -> Result<FivemClientSnapshot, FivemClientError> {
        if event.client_id != self.id {
            return Err(FivemClientError::WrongClientId {
                expected: self.id,
                actual: event.client_id,
            });
        }

        self.reconcile().await;

        {
            let mut state = self.state.write().await;

            if state.phase == FivemClientPhase::Stopped {
                return Ok(self.snapshot_from_state(&state).await);
            }

            match event.kind {
                ClientEventKind::SdkReady
                | ClientEventKind::GameLaunched
                | ClientEventKind::ConnectRequested => {
                    if state.phase != FivemClientPhase::Active {
                        state.phase = FivemClientPhase::Connecting;
                    }
                }
                ClientEventKind::ProcessState => {
                    if let Some(current) = event.current {
                        state.game_process_state = Some(current);
                    }
                }
                ClientEventKind::ConnectionState => {
                    if let Some(current) = event.current {
                        state.connection_state = Some(current);
                        state.phase = if current == 8 || event.active == Some(true) {
                            FivemClientPhase::Active
                        } else {
                            FivemClientPhase::Connecting
                        };
                    }
                }
                ClientEventKind::Heartbeat => {
                    if let Some(game_process_state) = event.game_process_state {
                        state.game_process_state = Some(game_process_state);
                    }
                    if let Some(connection_state) = event.connection_state {
                        state.connection_state = Some(connection_state);
                    }
                    if event.active == Some(true) || state.connection_state == Some(8) {
                        state.phase = FivemClientPhase::Active;
                    }
                }
            }
        }

        Ok(self.snapshot().await)
    }

    pub async fn snapshot(&self) -> FivemClientSnapshot {
        self.reconcile().await;
        let process = self.supervisor.snapshot().await;
        let state = self.state.read().await;

        FivemClientSnapshot {
            id: self.id,
            phase: state.phase,
            pid: state.pid,
            exit_code: state.exit_code,
            last_error: state.last_error.clone(),
            connection_state: state.connection_state,
            game_process_state: state.game_process_state,
            log_tail: process.log_tail,
        }
    }

    async fn snapshot_from_state(
        &self,
        state: &FivemClientState,
    ) -> FivemClientSnapshot {
        let process = self.supervisor.snapshot().await;
        FivemClientSnapshot {
            id: self.id,
            phase: state.phase,
            pid: state.pid,
            exit_code: state.exit_code,
            last_error: state.last_error.clone(),
            connection_state: state.connection_state,
            game_process_state: state.game_process_state,
            log_tail: process.log_tail,
        }
    }

    async fn reconcile(&self) {
        let process = self.supervisor.snapshot().await;
        let phase = self.state.read().await.phase;

        if process.phase == ProcessPhase::Exited
            && !matches!(
                phase,
                FivemClientPhase::Stopped | FivemClientPhase::Crashed
            )
        {
            let message = process.exit_code.map_or_else(
                || "FiveM/FxDK exited unexpectedly".to_owned(),
                |code| format!("FiveM/FxDK exited unexpectedly with code {code}"),
            );
            self.set_crashed(process.exit_code, message).await;
        }
    }

    async fn set_crashed(&self, exit_code: Option<i32>, message: String) {
        let mut state = self.state.write().await;
        state.phase = FivemClientPhase::Crashed;
        state.pid = None;
        state.exit_code = exit_code;
        state.last_error = Some(message);
    }
}

pub fn build_fivem_spec(
    config: &AppConfig,
    sdk_root: &Path,
    client_id: u32,
) -> Result<ProcessSpec, FivemClientError> {
    let fivem_path = required_fivem_path(config)?;
    if !sdk_root.is_dir() || !sdk_root.join("fxmanifest.lua").is_file() {
        return Err(FivemClientError::InvalidSdkRoot(sdk_root.to_path_buf()));
    }

    let sdk_root_text = sdk_root.to_string_lossy().replace('\\', "/");
    let ui_url = format!(
        "{DEFAULT_RUNTIME_WEB_BASE_URL}/?client={client_id}&server={}",
        config.server_address
    );

    let args: Vec<OsString> = [
        "-fxdk".to_owned(),
        "+set".to_owned(),
        "sdk_root_path".to_owned(),
        sdk_root_text,
        "+set".to_owned(),
        "sdk_url".to_owned(),
        ui_url.clone(),
        "+set".to_owned(),
        "fxdk_agent_ui_url".to_owned(),
        ui_url,
        "+set".to_owned(),
        "fxdk_agent_server".to_owned(),
        config.server_address.clone(),
        "+set".to_owned(),
        "fxdk_agent_control_url".to_owned(),
        DEFAULT_CONTROL_API_BASE_URL.to_owned(),
        "+set".to_owned(),
        "fxdk_agent_client".to_owned(),
        client_id.to_string(),
    ]
    .into_iter()
    .map(OsString::from)
    .collect();

    Ok(ProcessSpec::new(fivem_path).args(args))
}

fn required_fivem_path(config: &AppConfig) -> Result<&Path, FivemClientError> {
    let path = config
        .fivem_path
        .as_deref()
        .ok_or(FivemClientError::MissingFiveMPath)?;
    if !path.is_file() {
        return Err(FivemClientError::InvalidFiveMPath(path.to_path_buf()));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, process};

    use fxdk_agent_config::AppConfig;

    use super::{
        ClientEventKind, FivemClientController, FivemClientError, FivemClientEvent,
        FivemClientPhase, build_fivem_spec,
    };

    fn test_sdk_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "fxdk-agent-fivem-client-test-{}-{name}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create sdk root");
        fs::write(root.join("fxmanifest.lua"), b"# test").expect("manifest");
        root
    }

    #[test]
    fn launch_spec_requires_fivem_path() {
        let sdk_root = test_sdk_root("missing-fivem");
        let error = build_fivem_spec(&AppConfig::default(), &sdk_root, 1)
            .expect_err("missing FiveM must fail");

        assert!(matches!(error, FivemClientError::MissingFiveMPath));
    }

    #[test]
    fn launch_spec_contains_fxdk_runtime_contract() {
        let sdk_root = test_sdk_root("launch-spec");
        let config = AppConfig {
            fivem_path: Some(PathBuf::from(r"C:\Windows\System32\cmd.exe")),
            server_address: "127.0.0.1:30120".to_owned(),
            ..AppConfig::default()
        };

        let spec = build_fivem_spec(&config, &sdk_root, 1).expect("build spec");
        let args = spec
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(spec.program, PathBuf::from(r"C:\Windows\System32\cmd.exe"));
        assert!(args.iter().any(|arg| arg == "-fxdk"));
        assert!(args.windows(2).any(|window| {
            window[0] == "sdk_root_path"
                && window[1].replace('\\', "/")
                    == sdk_root.to_string_lossy().replace('\\', "/")
        }));
        assert!(args.windows(2).any(|window| {
            window[0] == "fxdk_agent_server" && window[1] == "127.0.0.1:30120"
        }));
        assert!(args.windows(2).any(|window| {
            window[0] == "fxdk_agent_client" && window[1] == "1"
        }));
    }

    #[tokio::test]
    async fn active_connection_event_promotes_client_state() {
        let controller = FivemClientController::new(1);
        {
            let mut state = controller.state.write().await;
            state.phase = FivemClientPhase::Connecting;
            state.pid = Some(123);
        }

        let snapshot = controller
            .record_event(FivemClientEvent {
                client_id: 1,
                kind: ClientEventKind::ConnectionState,
                current: Some(8),
                previous: Some(7),
                active: Some(true),
                reason: None,
                server_address: None,
                game_process_state: None,
                connection_state: None,
            })
            .await
            .expect("record active event");

        assert_eq!(snapshot.phase, FivemClientPhase::Active);
        assert_eq!(snapshot.connection_state, Some(8));
    }

    #[tokio::test]
    async fn rejects_runtime_event_for_other_client() {
        let controller = FivemClientController::new(1);
        let error = controller
            .record_event(FivemClientEvent {
                client_id: 2,
                kind: ClientEventKind::Heartbeat,
                current: None,
                previous: None,
                active: Some(false),
                reason: None,
                server_address: None,
                game_process_state: Some(2),
                connection_state: Some(0),
            })
            .await
            .expect_err("wrong client id must fail");

        assert!(matches!(
            error,
            FivemClientError::WrongClientId {
                expected: 1,
                actual: 2
            }
        ));
    }
}
