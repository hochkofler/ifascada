//! Launching, watching and killing the `edge-agent` child process.
//!
//! This is the half of the supervisor that must keep working even when everything else
//! does. It deliberately knows nothing about central, HTTP or orders.

use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

/// Owns the Win32 job object handle so it is closed exactly once, when the supervisor
/// process ends -- which is precisely what triggers the kernel to kill the child tree.
#[cfg(windows)]
struct WinJob(HANDLE);

#[cfg(windows)]
impl Drop for WinJob {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

// The handle is just a kernel object reference; moving it between threads is safe.
#[cfg(windows)]
unsafe impl Send for WinJob {}

/// What to launch. Kept separate from the launching so tests can drive the real process
/// machinery with a harmless command instead of the agent.
#[derive(Debug, Clone)]
pub struct ChildSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    /// Where the child's stdout and stderr go. `None` discards them, which is what tests
    /// want; production points both at the same log files `run-edge.ps1` has always used.
    pub out_log: Option<PathBuf>,
    pub err_log: Option<PathBuf>,
}

impl ChildSpec {
    pub fn new(program: impl Into<OsString>, args: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        ChildSpec {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            out_log: None,
            err_log: None,
        }
    }
}

/// Owns at most one running child at a time.
pub struct AgentChild {
    spec: ChildSpec,
    current: Option<Child>,
    #[cfg(windows)]
    job: Option<WinJob>,
}

impl AgentChild {
    pub fn new(spec: ChildSpec) -> Self {
        AgentChild {
            spec,
            current: None,
            #[cfg(windows)]
            job: None,
        }
    }

    pub fn spawn(&mut self) -> Result<()> {
        // Never spawn over a live child: two agents with the same client_id fighting over
        // the same serial ports is one of the states this component exists to prevent.
        if self.is_running() {
            self.kill()?;
        }

        let mut cmd = Command::new(&self.spec.program);
        cmd.args(&self.spec.args);
        cmd.stdin(Stdio::null());
        cmd.stdout(Self::sink(self.spec.out_log.as_ref())?);
        cmd.stderr(Self::sink(self.spec.err_log.as_ref())?);

        let child = cmd
            .spawn()
            .with_context(|| format!("failed to launch {:?}", self.spec.program))?;

        #[cfg(windows)]
        {
            let job = self.ensure_job()?;
            let handle = child.as_raw_handle() as HANDLE;
            if unsafe { AssignProcessToJobObject(job, handle) } == 0 {
                // The child is already running; refusing to supervise it would orphan it,
                // which is worse than supervising it without the kill-on-close guarantee.
                // Loud, because the guarantee the design leans on is now absent.
                tracing::error!(
                    "could not enrol the agent in the supervisor job object (os error {});                      it will NOT be killed automatically if the supervisor dies",
                    std::io::Error::last_os_error()
                );
            }
        }

        self.current = Some(child);
        Ok(())
    }

    #[cfg(windows)]
    fn ensure_job(&mut self) -> Result<HANDLE> {
        if let Some(j) = &self.job {
            return Ok(j.0);
        }
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                anyhow::bail!("CreateJobObject failed: {}", std::io::Error::last_os_error());
            }
            let job = WinJob(handle);

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                anyhow::bail!(
                    "SetInformationJobObject failed: {}",
                    std::io::Error::last_os_error()
                );
            }
            self.job = Some(job);
            Ok(handle)
        }
    }

    /// Whether the running child belongs to this supervisor's job object.
    #[cfg(windows)]
    pub fn child_is_in_our_job(&self) -> Result<bool> {
        let (Some(job), Some(child)) = (&self.job, &self.current) else {
            anyhow::bail!("no job or no running child");
        };
        let mut inside: i32 = 0;
        let ok =
            unsafe { IsProcessInJob(child.as_raw_handle() as HANDLE, job.0, &mut inside) };
        if ok == 0 {
            anyhow::bail!("IsProcessInJob failed: {}", std::io::Error::last_os_error());
        }
        Ok(inside != 0)
    }

    /// Whether the job carries the limit that makes the kernel kill its processes when the
    /// last handle to it closes.
    #[cfg(windows)]
    pub fn job_kills_on_close(&self) -> Result<bool> {
        let Some(job) = &self.job else {
            anyhow::bail!("no job object");
        };
        unsafe {
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            let mut returned: u32 = 0;
            let ok = QueryInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &mut info as *mut _ as *mut core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                &mut returned,
            );
            if ok == 0 {
                anyhow::bail!(
                    "QueryInformationJobObject failed: {}",
                    std::io::Error::last_os_error()
                );
            }
            Ok(info.BasicLimitInformation.LimitFlags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE != 0)
        }
    }

    /// True while the child is alive. Reaps it if it has exited, so a later `spawn`
    /// does not leave a zombie behind on Unix.
    pub fn is_running(&mut self) -> bool {
        let Some(child) = self.current.as_mut() else {
            return false;
        };
        match child.try_wait() {
            Ok(None) => true,
            // Exited, or we can no longer ask about it. Either way it is not ours to
            // watch any more, and holding the handle would only leak it.
            Ok(Some(_)) | Err(_) => {
                self.current = None;
                false
            }
        }
    }

    pub fn kill(&mut self) -> Result<()> {
        // Terminating the job takes the agent's own children with it. `run-edge.ps1` never
        // did this, so a killed agent could leave grandchildren behind holding COM ports.
        #[cfg(windows)]
        if let Some(job) = &self.job {
            unsafe { TerminateJobObject(job.0, 1) };
        }

        if let Some(mut child) = self.current.take() {
            // A child that already exited makes kill fail; that is the desired end state,
            // so it is not an error worth propagating. The wait reaps it either way.
            let _ = child.kill();
            let _ = child.wait();
        }
        Ok(())
    }

    pub fn pid(&self) -> Option<u32> {
        self.current.as_ref().map(|c| c.id())
    }

    /// Appends to the log file when there is one, discards otherwise. Appending rather
    /// than truncating keeps the history across the restarts this component performs.
    fn sink(path: Option<&PathBuf>) -> Result<Stdio> {
        match path {
            None => Ok(Stdio::null()),
            Some(p) => {
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                let f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(p)
                    .with_context(|| format!("failed to open log {}", p.display()))?;
                Ok(Stdio::from(f))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A command that stays alive long enough to be observed and killed.
    fn sleeper() -> ChildSpec {
        #[cfg(windows)]
        {
            ChildSpec::new("cmd", ["/c", "ping", "-n", "30", "127.0.0.1"])
        }
        #[cfg(not(windows))]
        {
            ChildSpec::new("sh", ["-c", "sleep 30"])
        }
    }

    /// A command that exits on its own almost immediately.
    fn quitter() -> ChildSpec {
        #[cfg(windows)]
        {
            ChildSpec::new("cmd", ["/c", "exit", "0"])
        }
        #[cfg(not(windows))]
        {
            ChildSpec::new("sh", ["-c", "exit 0"])
        }
    }

    /// Polls a condition instead of sleeping a fixed amount, so the test is neither flaky
    /// on a loaded machine nor slower than it has to be on an idle one.
    fn wait_until(mut cond: impl FnMut() -> bool, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        cond()
    }

    #[test]
    fn a_spawned_child_is_running_and_has_a_pid() {
        let mut child = AgentChild::new(sleeper());
        child.spawn().expect("spawn failed");

        assert!(child.is_running(), "the child should be alive");
        assert!(child.pid().is_some(), "a running child must report a pid");

        let _ = child.kill();
    }

    #[test]
    fn killing_the_child_stops_it() {
        let mut child = AgentChild::new(sleeper());
        child.spawn().expect("spawn failed");
        assert!(child.is_running());

        child.kill().expect("kill failed");

        assert!(
            wait_until(|| !child.is_running(), Duration::from_secs(5)),
            "the child should be gone after kill"
        );
        assert_eq!(child.pid(), None, "a dead child must not report a pid");
    }

    /// The case `run-edge.ps1` already handled: the agent exits by itself and has to be
    /// noticed so it can be relaunched.
    #[test]
    fn a_child_that_exits_on_its_own_is_noticed() {
        let mut child = AgentChild::new(quitter());
        child.spawn().expect("spawn failed");

        assert!(
            wait_until(|| !child.is_running(), Duration::from_secs(5)),
            "an exited child must stop reporting as running"
        );
    }

    #[test]
    fn a_child_can_be_relaunched_after_it_died() {
        let mut child = AgentChild::new(quitter());
        child.spawn().expect("first spawn failed");
        assert!(wait_until(|| !child.is_running(), Duration::from_secs(5)));

        child.spawn().expect("relaunch failed");
        assert!(child.pid().is_some(), "the relaunched child needs a pid");

        let _ = child.kill();
    }

    /// Spawning over a live child would leak the old one: two agents with the same
    /// client_id fighting over the same serial ports is exactly the state the supervisor
    /// exists to prevent.
    #[test]
    fn spawning_while_a_child_is_alive_does_not_leak_it() {
        let mut child = AgentChild::new(sleeper());
        child.spawn().expect("spawn failed");
        let first = child.pid().expect("first pid");

        child.spawn().expect("second spawn failed");
        let second = child.pid().expect("second pid");

        assert_ne!(first, second, "a new child should have been started");
        assert!(
            !process_is_alive(first),
            "the previous child ({}) must have been killed, not orphaned",
            first
        );

        let _ = child.kill();
    }

    #[cfg(windows)]
    fn process_is_alive(pid: u32) -> bool {
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/NH"])
            .output()
            .expect("tasklist failed");
        String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
    }

    #[cfg(not(windows))]
    fn process_is_alive(pid: u32) -> bool {
        std::path::Path::new(&format!("/proc/{}", pid)).exists()
    }

    /// The hard requirement from the design: if the supervisor dies, the agent must die
    /// with it. An orphaned agent would fight the next one over the same serial ports and
    /// the same MQTT client_id, and it would also fight `update-edge.ps1`, which stops the
    /// scheduled task and then replaces a binary it expects nobody to be running.
    ///
    /// On Windows the mechanism is a Job Object with KILL_ON_JOB_CLOSE: the handle closes
    /// when the supervisor process ends, however it ends, and the kernel takes the whole
    /// tree down. This asserts the child really is inside *our* job -- the property the
    /// guarantee rests on.
    #[cfg(windows)]
    #[test]
    fn the_child_is_enrolled_in_a_kill_on_close_job() {
        let mut child = AgentChild::new(sleeper());
        child.spawn().expect("spawn failed");

        assert!(
            child.child_is_in_our_job().expect("job membership query failed"),
            "the child must belong to the supervisor's job object"
        );
        assert!(
            child.job_kills_on_close().expect("job limit query failed"),
            "the job must carry JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE"
        );

        let _ = child.kill();
    }

}
