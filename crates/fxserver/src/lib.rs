use std::{
    error::Error,
    ffi::OsString,
    fmt, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use fxdk_agent_config::AppConfig;
use fxdk_agent_process_supervisor::{
    ProcessPhase, ProcessSpec, ProcessSupervisor,
};
use tokio::{
    net::TcpStream,
    sync::{Mutex, RwLock},
    time::{Instant, sleep, timeout},
};

const DEFAULT_READINESS_TIMEOUT: Duration = Duration::from_secs(30);
const READINESS_POLL_INTERVAL: Duration = Duration::from_millis(200);
const CONNECT_ATTEMPT_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxServerPhase {
    Stopped,
    Starting,
    Online,
    Stopping,
    Crashed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FxServerSnapshot {
    pub phase: FxServerPhase,
    pub pid: Option<u32>,
    pub address: Option<String>,
    pub exit_code: Option<i32>,
    pub last_error: Option<String>,
    pub log_tail: Vec<String>,
}

#[derive(Debug)]
struct FxServerState {
    phase: FxServerPhase,
    pid: Option<u32>,
    address: Option<String>,
    exit_code: Option<i32>,
    last_error: Option<String>,
}

impl Default for FxServerState {
    fn default() -> Self {
        Self {
            phase: FxServerPhase::Stopped,
            pid: None,
            address: None,
            exit_code: None,
            last_error: None,
        }
    }
}

#[derive(Debug)]
pub enum FxServerError {
    MissingServerProject,
    MissingFxServerPath,
    MissingServerConfig(PathBuf),
    InvalidServerProject(PathBuf),
    InvalidFxServerPath(PathBuf),
    AlreadyRunning,
    Spawn(io::Error),
    EarlyExit(Option<i32>),
    ReadinessTimeout(String),
    Stop(io::Error),
}

impl fmt::Display for FxServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingServerProject => write!(formatter, "server project is not configured"),
            Self::MissingFxServerPath => write!(formatter, "FXServer executable is not configured"),
            Self::MissingServerConfig(path) => {
                write!(formatter, "server.cfg was not found at {}", path.display())
            }
            Self::InvalidServerProject(path) => {
                write!(formatter, "server project is not a directory: {}", path.display())
            }
            Self::InvalidFxServerPath(path) => {
                write!(formatter, "FXServer executable is not a file: {}", path.display())
            }
            Self::AlreadyRunning => write!(formatter, "FXServer is already running"),
            Self::Spawn(error) => write!(formatter, "failed to spawn FXServer: {error}"),
            Self::EarlyExit(code) => write!(
                formatter,
                "FXServer exited before readiness{}",
                code.map_or_else(String::new, |code| format!(" with code {code}"))
            ),
            Self::ReadinessTimeout(address) => {
                write!(formatter, "FXServer did not become ready at {address}")
            }
            Self::Stop(error) => write!(formatter, "failed to stop FXServer: {error}"),
        }
    }
}

impl Error for FxServerError {}

#[derive(Clone)]
pub struct FxServerController {
    supervisor: ProcessSupervisor,
    state: Arc<RwLock<FxServerState>>,
    operation: Arc<Mutex<()>>,
    readiness_timeout: Duration,
}

impl Default for FxServerController {
    fn default() -> Self {
        Self::new(DEFAULT_READINESS_TIMEOUT)
    }
}

impl FxServerController {
    pub fn new(readiness_timeout: Duration) -> Self {
        Self {
            supervisor: ProcessSupervisor::default(),
            state: Arc::new(RwLock::new(FxServerState::default())),
            operation: Arc::new(Mutex::new(())),
            readiness_timeout,
        }
    }

    pub async fn start(
        &self,
        config: &AppConfig,
    ) -> Result<FxServerSnapshot, FxServerError> {
        let _operation = self.operation.lock().await;
        self.reconcile().await;

        {
            let state = self.state.read().await;
            if matches!(state.phase, FxServerPhase::Starting | FxServerPhase::Online) {
                return Err(FxServerError::AlreadyRunning);
            }
        }

        let spec = build_fxserver_spec(config)?;

        {
            let mut state = self.state.write().await;
            state.phase = FxServerPhase::Starting;
            state.address = Some(config.server_address.clone());
            state.exit_code = None;
            state.last_error = None;
        }

        let process = match self.supervisor.spawn(spec).await {
            Ok(process) => process,
            Err(error) => {
                let message = error.to_string();
                self.set_crashed(None, message.clone()).await;
                return Err(FxServerError::Spawn(error));
            }
        };

        {
            let mut state = self.state.write().await;
            state.pid = process.pid;
        }

        match self.wait_until_ready(&config.server_address).await {
            Ok(()) => {
                let mut state = self.state.write().await;
                state.phase = FxServerPhase::Online;
                state.last_error = None;
                drop(state);
                Ok(self.snapshot().await)
            }
            Err(error) => {
                let _ = self.supervisor.stop().await;
                let process = self.supervisor.snapshot().await;
                let message = error.to_string();
                self.set_crashed(process.exit_code, message).await;
                Err(error)
            }
        }
    }

    pub async fn stop(&self) -> Result<FxServerSnapshot, FxServerError> {
        let _operation = self.operation.lock().await;
        self.reconcile().await;

        {
            let state = self.state.read().await;
            if state.phase == FxServerPhase::Stopped {
                drop(state);
                return Ok(self.snapshot().await);
            }
        }

        {
            let mut state = self.state.write().await;
            state.phase = FxServerPhase::Stopping;
        }

        let process = self
            .supervisor
            .stop()
            .await
            .map_err(FxServerError::Stop)?;

        {
            let mut state = self.state.write().await;
            state.phase = FxServerPhase::Stopped;
            state.pid = None;
            state.exit_code = process.exit_code;
            state.last_error = None;
        }

        Ok(self.snapshot().await)
    }

    pub async fn snapshot(&self) -> FxServerSnapshot {
        self.reconcile().await;
        let process = self.supervisor.snapshot().await;
        let state = self.state.read().await;

        FxServerSnapshot {
            phase: state.phase,
            pid: state.pid,
            address: state.address.clone(),
            exit_code: state.exit_code,
            last_error: state.last_error.clone(),
            log_tail: process.log_tail,
        }
    }

    async fn wait_until_ready(&self, address: &str) -> Result<(), FxServerError> {
        let deadline = Instant::now() + self.readiness_timeout;

        loop {
            if timeout(CONNECT_ATTEMPT_TIMEOUT, TcpStream::connect(address))
                .await
                .is_ok_and(|result| result.is_ok())
            {
                return Ok(());
            }

            let process = self.supervisor.snapshot().await;
            if process.phase == ProcessPhase::Exited {
                return Err(FxServerError::EarlyExit(process.exit_code));
            }

            if Instant::now() >= deadline {
                return Err(FxServerError::ReadinessTimeout(address.to_owned()));
            }

            sleep(READINESS_POLL_INTERVAL).await;
        }
    }

    async fn reconcile(&self) {
        let process = self.supervisor.snapshot().await;
        let phase = self.state.read().await.phase;

        if process.phase == ProcessPhase::Exited
            && !matches!(phase, FxServerPhase::Stopped | FxServerPhase::Crashed)
        {
            let message = process.exit_code.map_or_else(
                || "FXServer exited unexpectedly".to_owned(),
                |code| format!("FXServer exited unexpectedly with code {code}"),
            );
            self.set_crashed(process.exit_code, message).await;
        }
    }

    async fn set_crashed(&self, exit_code: Option<i32>, message: String) {
        let mut state = self.state.write().await;
        state.phase = FxServerPhase::Crashed;
        state.pid = None;
        state.exit_code = exit_code;
        state.last_error = Some(message);
    }
}

pub fn build_fxserver_spec(config: &AppConfig) -> Result<ProcessSpec, FxServerError> {
    let server_project = required_server_project(config)?;
    let fxserver_path = required_fxserver_path(config)?;
    let server_cfg = server_project.join("server.cfg");

    if !server_cfg.is_file() {
        return Err(FxServerError::MissingServerConfig(server_cfg));
    }

    let args: Vec<OsString> = [
        "+set",
        "sv_lan",
        "1",
        "+set",
        "onesync",
        "on",
        "+set",
        "sv_fxdkMode",
        "1",
        "+exec",
        "server.cfg",
        "+set",
        "sv_lan",
        "1",
        "+set",
        "onesync",
        "on",
        "+set",
        "sv_fxdkMode",
        "1",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();

    Ok(ProcessSpec::new(fxserver_path)
        .args(args)
        .current_dir(server_project))
}

fn required_server_project(config: &AppConfig) -> Result<&Path, FxServerError> {
    let path = config
        .server_project
        .as_deref()
        .ok_or(FxServerError::MissingServerProject)?;
    if !path.is_dir() {
        return Err(FxServerError::InvalidServerProject(path.to_path_buf()));
    }
    Ok(path)
}

fn required_fxserver_path(config: &AppConfig) -> Result<&Path, FxServerError> {
    let path = config
        .fxserver_path
        .as_deref()
        .ok_or(FxServerError::MissingFxServerPath)?;
    if !path.is_file() {
        return Err(FxServerError::InvalidFxServerPath(path.to_path_buf()));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::{fs, process};

    use fxdk_agent_config::AppConfig;

    use super::{
        FxServerError, build_fxserver_spec,
    };

    #[test]
    fn launch_spec_requires_configured_paths() {
        let error = build_fxserver_spec(&AppConfig::default())
            .expect_err("missing project must fail");
        assert!(matches!(error, FxServerError::MissingServerProject));
    }

    #[test]
    fn launch_spec_uses_server_project_and_dev_convars() {
        let root = std::env::temp_dir().join(format!(
            "fxdk-agent-fxserver-spec-{}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let project = root.join("server-data");
        fs::create_dir_all(&project).expect("project");
        fs::write(project.join("server.cfg"), b"# test").expect("server cfg");
        let executable = root.join("FXServer.exe");
        fs::write(&executable, b"test").expect("fake executable");

        let config = AppConfig {
            server_project: Some(project.clone()),
            fxserver_path: Some(executable.clone()),
            ..AppConfig::default()
        };

        let spec = build_fxserver_spec(&config).expect("build spec");

        assert_eq!(spec.program, executable);
        assert_eq!(spec.cwd.as_deref(), Some(project.as_path()));
        let args = spec
            .args
            .iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>();
        assert!(args.windows(3).any(|window| {
            window == ["+set", "sv_lan", "1"]
        }));
        assert!(args.windows(2).any(|window| {
            window == ["+exec", "server.cfg"]
        }));
    }
}
