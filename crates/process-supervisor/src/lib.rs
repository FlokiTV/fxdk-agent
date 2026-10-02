use std::{
    collections::VecDeque, ffi::OsString, io, path::PathBuf, process::Stdio, sync::Arc,
    time::Duration,
};

#[cfg(windows)]
use process_wrap::tokio::JobObject;
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    sync::{Mutex, RwLock},
    time::{Instant, sleep},
};

const DEFAULT_LOG_TAIL_LINES: usize = 200;
const MONITOR_INTERVAL: Duration = Duration::from_millis(250);
const STOP_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
}

impl ProcessSpec {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn current_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.cwd = Some(path.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessPhase {
    Stopped,
    Running,
    Stopping,
    Exited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSnapshot {
    pub phase: ProcessPhase,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub log_tail: Vec<String>,
}

#[derive(Debug)]
struct ProcessState {
    phase: ProcessPhase,
    pid: Option<u32>,
    exit_code: Option<i32>,
}

impl Default for ProcessState {
    fn default() -> Self {
        Self {
            phase: ProcessPhase::Stopped,
            pid: None,
            exit_code: None,
        }
    }
}

struct Inner {
    child: Mutex<Option<Box<dyn ChildWrapper>>>,
    state: RwLock<ProcessState>,
    logs: Mutex<VecDeque<String>>,
    log_capacity: usize,
}

#[derive(Clone)]
pub struct ProcessSupervisor {
    inner: Arc<Inner>,
}

impl Default for ProcessSupervisor {
    fn default() -> Self {
        Self::new(DEFAULT_LOG_TAIL_LINES)
    }
}

impl ProcessSupervisor {
    pub fn new(log_capacity: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                child: Mutex::new(None),
                state: RwLock::new(ProcessState::default()),
                logs: Mutex::new(VecDeque::with_capacity(log_capacity)),
                log_capacity,
            }),
        }
    }

    pub async fn spawn(&self, spec: ProcessSpec) -> io::Result<ProcessSnapshot> {
        let mut child_slot = self.inner.child.lock().await;
        if child_slot.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "managed process is already running",
            ));
        }

        self.clear_logs().await;

        let mut command = CommandWrap::with_new(&spec.program, |command| {
            command.args(&spec.args);
            if let Some(cwd) = &spec.cwd {
                command.current_dir(cwd);
            }
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());
        });

        #[cfg(windows)]
        command.wrap(JobObject);
        command.wrap(KillOnDrop);

        let mut child = command.spawn()?;
        let pid = child
            .id()
            .ok_or_else(|| io::Error::other("spawned process did not expose a process id"))?;

        if let Some(stdout) = child.stdout().take() {
            self.capture_output("stdout", stdout);
        }
        if let Some(stderr) = child.stderr().take() {
            self.capture_output("stderr", stderr);
        }

        *child_slot = Some(child);
        drop(child_slot);

        {
            let mut state = self.inner.state.write().await;
            state.phase = ProcessPhase::Running;
            state.pid = Some(pid);
            state.exit_code = None;
        }

        self.start_monitor();
        Ok(self.snapshot().await)
    }

    pub async fn stop(&self) -> io::Result<ProcessSnapshot> {
        {
            let mut state = self.inner.state.write().await;
            if state.phase == ProcessPhase::Stopped {
                return Ok(self.snapshot_from_state(&state).await);
            }
            state.phase = ProcessPhase::Stopping;
        }

        let mut child_slot = self.inner.child.lock().await;
        if let Some(child) = child_slot.as_mut() {
            child.start_kill()?;

            let deadline = Instant::now() + STOP_WAIT_TIMEOUT;
            let exit_code = loop {
                if let Some(status) = child.try_wait()? {
                    break status.code();
                }

                if Instant::now() >= deadline {
                    self.push_log(
                        "[supervisor] stop wait timed out; dropping managed job handle".to_owned(),
                    )
                    .await;
                    break None;
                }

                sleep(STOP_POLL_INTERVAL).await;
            };

            let mut state = self.inner.state.write().await;
            state.phase = ProcessPhase::Stopped;
            state.pid = None;
            state.exit_code = exit_code;
        } else {
            let mut state = self.inner.state.write().await;
            state.phase = ProcessPhase::Stopped;
            state.pid = None;
        }

        // Dropping the JobObject child closes the managed job. With KillOnDrop enabled
        // this is also the bounded fallback for descendants that do not report exit.
        *child_slot = None;
        drop(child_slot);

        Ok(self.snapshot().await)
    }

    pub async fn snapshot(&self) -> ProcessSnapshot {
        let state = self.inner.state.read().await;
        self.snapshot_from_state(&state).await
    }

    async fn snapshot_from_state(&self, state: &ProcessState) -> ProcessSnapshot {
        let logs = self.inner.logs.lock().await;
        ProcessSnapshot {
            phase: state.phase,
            pid: state.pid,
            exit_code: state.exit_code,
            log_tail: logs.iter().cloned().collect(),
        }
    }

    fn capture_output<R>(&self, stream: &'static str, reader: R)
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let supervisor = self.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => supervisor.push_log(format!("[{stream}] {line}")).await,
                    Ok(None) => break,
                    Err(error) => {
                        supervisor
                            .push_log(format!("[{stream}] <read error: {error}>"))
                            .await;
                        break;
                    }
                }
            }
        });
    }

    fn start_monitor(&self) {
        let supervisor = self.clone();
        tokio::spawn(async move {
            loop {
                sleep(MONITOR_INTERVAL).await;

                let exit_status = {
                    let mut child_slot = supervisor.inner.child.lock().await;
                    let Some(child) = child_slot.as_mut() else {
                        break;
                    };

                    match child.try_wait() {
                        Ok(Some(status)) => {
                            *child_slot = None;
                            Some(Ok(status.code()))
                        }
                        Ok(None) => None,
                        Err(error) => {
                            *child_slot = None;
                            Some(Err(error))
                        }
                    }
                };

                match exit_status {
                    None => continue,
                    Some(Ok(exit_code)) => {
                        let mut state = supervisor.inner.state.write().await;
                        let was_stopping = state.phase == ProcessPhase::Stopping;
                        state.phase = if was_stopping {
                            ProcessPhase::Stopped
                        } else {
                            ProcessPhase::Exited
                        };
                        state.pid = None;
                        state.exit_code = exit_code;
                        break;
                    }
                    Some(Err(error)) => {
                        supervisor
                            .push_log(format!("[supervisor] process status error: {error}"))
                            .await;
                        let mut state = supervisor.inner.state.write().await;
                        state.phase = ProcessPhase::Exited;
                        state.pid = None;
                        state.exit_code = None;
                        break;
                    }
                }
            }
        });
    }

    async fn clear_logs(&self) {
        self.inner.logs.lock().await.clear();
    }

    async fn push_log(&self, line: String) {
        let mut logs = self.inner.logs.lock().await;
        if self.inner.log_capacity == 0 {
            return;
        }
        while logs.len() >= self.inner.log_capacity {
            logs.pop_front();
        }
        logs.push_back(line);
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, process, time::Duration};

    use tokio::time::{sleep, timeout};

    use super::{ProcessPhase, ProcessSpec, ProcessSupervisor};

    #[tokio::test]
    async fn captures_output_and_observes_normal_exit() {
        let supervisor = ProcessSupervisor::new(20);
        let spec = ProcessSpec::new("cmd.exe").args(["/C", "echo fxdk-agent-supervisor"]);

        let started = supervisor.spawn(spec).await.expect("spawn command");
        assert_eq!(started.phase, ProcessPhase::Running);
        assert!(started.pid.is_some());

        timeout(Duration::from_secs(5), async {
            loop {
                let snapshot = supervisor.snapshot().await;
                if snapshot.phase == ProcessPhase::Exited {
                    assert_eq!(snapshot.exit_code, Some(0));
                    assert!(
                        snapshot
                            .log_tail
                            .iter()
                            .any(|line| { line.contains("fxdk-agent-supervisor") })
                    );
                    break;
                }
                sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("process exit");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn stop_terminates_windows_job_descendants() {
        let supervisor = ProcessSupervisor::new(20);
        let root = std::env::temp_dir().join(format!("fxdk-agent-job-test-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("test root");
        let pid_file = root.join("child.pid");
        let escaped_pid_file = pid_file.display().to_string().replace('\'', "''");

        let script = format!(
            "$child = Start-Process powershell -ArgumentList '-NoProfile','-Command','Start-Sleep -Seconds 60' -PassThru -WindowStyle Hidden; Set-Content -Path '{escaped_pid_file}' -Value $child.Id; Wait-Process -Id $child.Id"
        );
        let spec = ProcessSpec::new("powershell.exe")
            .args(["-NoProfile", "-Command"])
            .arg(script);

        let started = supervisor.spawn(spec).await.expect("spawn process tree");
        let parent_pid = started.pid.expect("parent pid");

        let child_pid = timeout(Duration::from_secs(10), async {
            loop {
                if let Ok(text) = fs::read_to_string(&pid_file)
                    && let Ok(pid) = text.trim().parse::<u32>()
                {
                    break pid;
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("child pid file");

        supervisor.stop().await.expect("stop process tree");

        timeout(Duration::from_secs(5), async {
            loop {
                if !windows_process_exists(parent_pid) && !windows_process_exists(child_pid) {
                    break;
                }
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("job descendants terminated");

        let snapshot = supervisor.snapshot().await;
        assert_eq!(snapshot.phase, ProcessPhase::Stopped);
        assert_eq!(snapshot.pid, None);
    }

    #[cfg(windows)]
    fn windows_process_exists(pid: u32) -> bool {
        std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}"
                ),
            ])
            .status()
            .is_ok_and(|status| status.success())
    }
}
