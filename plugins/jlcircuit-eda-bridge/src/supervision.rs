//! Supervised lifecycle for the local `circuitfabric-jlc-bridge` process.
//!
//! The desktop control plane starts and stops the bridge as a child process.
//! The child is placed in a kill-on-close job so it cannot outlive the app,
//! and its (tiny) stderr is kept readable so an early exit can be explained.

use std::{
    env,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
};

use circuitfabric_codex_runtime::execution::{ProcessOwnership, stop_child};

const BRIDGE_BINARY_NAME: &str = "circuitfabric-jlc-bridge";

/// Locates the bridge executable: next to the running executable first (the
/// usual cargo `target/<profile>/` layout), then on `PATH`.
///
/// # Errors
///
/// Returns an error naming the searched locations when the binary is nowhere
/// to be found.
pub fn resolve_bridge_binary() -> Result<PathBuf, String> {
    let file_name = if cfg!(windows) {
        format!("{BRIDGE_BINARY_NAME}.exe")
    } else {
        BRIDGE_BINARY_NAME.to_owned()
    };
    if let Ok(current_exe) = env::current_exe() {
        let candidate = current_exe.parent().map(|dir| dir.join(&file_name));
        if let Some(found) = candidate.filter(|path| path.is_file()) {
            return Ok(found);
        }
    }
    if let Some(search) = env::var_os("PATH") {
        for dir in env::split_paths(&search) {
            let candidate = dir.join(&file_name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(format!(
        "未找到 {BRIDGE_BINARY_NAME} 可执行文件（已查找应用目录与 PATH）；请先构建 plugins/jlcircuit-eda-bridge"
    ))
}

/// A supervised bridge child process owned by the desktop control plane.
#[derive(Debug)]
pub struct BridgeProcessHandle {
    child: Child,
    ownership: Option<ProcessOwnership>,
}

impl BridgeProcessHandle {
    /// Starts the bridge with the resolved binary and the given runtime settings path.
    ///
    /// # Errors
    ///
    /// Returns an error when the binary cannot be resolved or spawned.
    pub fn launch(config_path: &Path) -> Result<Self, String> {
        Self::launch_binary(&resolve_bridge_binary()?, config_path)
    }

    /// Starts the bridge from an explicit executable path (used by tests and
    /// by callers that manage the binary location themselves).
    ///
    /// # Errors
    ///
    /// Returns an error when the process cannot be spawned or supervised.
    pub fn launch_binary(binary: &Path, config_path: &Path) -> Result<Self, String> {
        let mut child = Command::new(binary)
            .arg(config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("启动 bridge 失败（{}）：{error}", binary.display()))?;
        let ownership = ProcessOwnership::attach(&mut child)
            .map_err(|error| format!("无法监管 bridge 进程：{error}"))?;
        Ok(Self { child, ownership: Some(ownership) })
    }

    /// Process identifier of the supervised bridge.
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Non-blocking exit poll: `None` while the bridge is still running.
    pub fn try_exit(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    /// Reads the bridge's buffered stderr (capped) for exit diagnostics.
    /// Call only after [`Self::try_exit`] reports the process has terminated.
    pub fn exit_diagnostics(&mut self) -> String {
        let mut text = String::new();
        if let Some(stderr) = self.child.stderr.as_mut() {
            let _ = stderr.take(8192).read_to_string(&mut text);
        }
        text.trim().to_owned()
    }

    /// Terminates the bridge and its process tree. An already-exited process
    /// is reported as a successful stop.
    ///
    /// # Errors
    ///
    /// Returns an error when the process cannot be killed or reaped.
    pub fn stop(&mut self) -> Result<(), String> {
        self.ownership.take();
        stop_child(&mut self.child);
        self.child.wait().map(|_| ()).map_err(|error| format!("停止 bridge 失败：{error}"))
    }
}

impl Drop for BridgeProcessHandle {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
