use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

use crate::error::OpenCodeError;

/// Level of OS-level process containment enforcement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SandboxLevel {
    /// Full OS-level sandbox containment with memory, process, and kill-on-close limits.
    /// Strictly fails closed if the OS does not support Job Objects or cgroups.
    Strict,
    /// Standard Job Object with kill-on-close semantics. Fails closed if unsupported.
    #[default]
    Standard,
    /// Containment disabled (unrestricted host execution). Only permitted when explicitly configured.
    None,
}

impl SandboxLevel {
    /// Returns true if this containment level mandates OS-level sandboxing primitives.
    pub fn requires_os_containment(&self) -> bool {
        matches!(self, Self::Strict | Self::Standard)
    }
}

/// Configuration policy for child process containment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxPolicy {
    pub level: SandboxLevel,
    /// Whether to enforce Win32 Job Object containment.
    #[serde(default = "default_enforce_job_object")]
    pub enforce_job_object: bool,
    /// Maximum virtual memory allocation in bytes for the entire job/process tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_memory_bytes: Option<u64>,
    /// Maximum number of active processes allowed within the containment tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_processes: Option<u32>,
    /// Whether closing the supervisor job handle immediately terminates all child processes.
    #[serde(default = "default_kill_on_close")]
    pub kill_on_close: bool,
}

fn default_enforce_job_object() -> bool {
    true
}

fn default_kill_on_close() -> bool {
    true
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            level: SandboxLevel::Standard,
            enforce_job_object: true,
            max_memory_bytes: Some(4 * 1024 * 1024 * 1024), // 4 GiB
            max_processes: Some(64),
            kill_on_close: true,
        }
    }
}

impl SandboxPolicy {
    pub fn strict() -> Self {
        Self {
            level: SandboxLevel::Strict,
            enforce_job_object: true,
            max_memory_bytes: Some(2 * 1024 * 1024 * 1024), // 2 GiB
            max_processes: Some(16),
            kill_on_close: true,
        }
    }

    pub fn standard() -> Self {
        Self::default()
    }

    pub fn unconstrained() -> Self {
        Self {
            level: SandboxLevel::None,
            enforce_job_object: false,
            max_memory_bytes: None,
            max_processes: None,
            kill_on_close: false,
        }
    }

    pub fn disabled() -> Self {
        Self::unconstrained()
    }

    pub fn with_max_memory_mb(mut self, mb: u64) -> Self {
        self.max_memory_bytes = Some(mb * 1024 * 1024);
        self
    }

    pub fn with_memory_limit(mut self, bytes: u64) -> Self {
        self.max_memory_bytes = Some(bytes);
        self
    }

    pub fn with_max_processes(mut self, count: u32) -> Self {
        self.max_processes = Some(count);
        self
    }

    pub fn with_process_limit(mut self, count: u32) -> Self {
        self.max_processes = Some(count);
        self
    }
}

/// Active process containment supervisor.
#[derive(Clone)]
pub struct ProcessContainment {
    policy: SandboxPolicy,
    #[cfg(windows)]
    job_object: Option<Arc<JobObjectGuard>>,
}

impl ProcessContainment {
    /// Initializes containment according to the provided policy.
    /// Fails closed if the policy requires OS-level containment and the platform or API cannot supply it.
    pub fn new(policy: SandboxPolicy) -> Result<Self, OpenCodeError> {
        match policy.level {
            SandboxLevel::None => {
                warn!(
                    "Process containment is explicitly DISABLED. OpenCode processes will run with host privileges."
                );
                Ok(Self {
                    policy,
                    #[cfg(windows)]
                    job_object: None,
                })
            }
            #[cfg(windows)]
            SandboxLevel::Strict | SandboxLevel::Standard => {
                let job = JobObjectGuard::create(&policy)?;
                info!(
                    level = ?policy.level,
                    kill_on_close = policy.kill_on_close,
                    max_memory_bytes = ?policy.max_memory_bytes,
                    max_processes = ?policy.max_processes,
                    "Windows Job Object containment initialized successfully"
                );
                Ok(Self {
                    policy,
                    job_object: Some(Arc::new(job)),
                })
            }
            #[cfg(not(windows))]
            SandboxLevel::Strict | SandboxLevel::Standard => {
                Err(OpenCodeError::UnsupportedPlatform(format!(
                    "Sandbox level {:?} requires OS-level containment. On this platform, Windows Job Objects are unavailable and Linux cgroups are not yet configured. Failing closed.",
                    policy.level
                )))
            }
        }
    }

    pub fn from_policy(policy: &SandboxPolicy) -> Result<Self, OpenCodeError> {
        Self::new(policy.clone())
    }

    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    /// Assigns a newly spawned child process to the containment environment.
    pub fn assign_child(&self, pid: u32) -> Result<(), OpenCodeError> {
        #[cfg(windows)]
        {
            if let Some(ref job) = self.job_object {
                job.assign_pid(pid)?;
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            if self.policy.level.requires_os_containment() {
                Err(OpenCodeError::SandboxPolicyViolation(
                    "Cannot assign process: OS containment required but unavailable on platform"
                        .into(),
                ))
            } else {
                Ok(())
            }
        }
    }

    pub fn assign_process(&self, pid: u32) -> Result<(), OpenCodeError> {
        self.assign_child(pid)
    }

    /// Terminates all processes within the containment tree.
    pub fn terminate(&self) -> Result<(), OpenCodeError> {
        #[cfg(windows)]
        {
            if let Some(ref job) = self.job_object {
                job.terminate(1)?;
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            Ok(())
        }
    }
}

#[cfg(windows)]
pub struct JobObjectGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
unsafe impl Send for JobObjectGuard {}
#[cfg(windows)]
unsafe impl Sync for JobObjectGuard {}

#[cfg(windows)]
impl JobObjectGuard {
    pub fn create(policy: &SandboxPolicy) -> Result<Self, OpenCodeError> {
        use std::mem::size_of;
        use std::ptr::null;
        use windows_sys::Win32::Foundation::GetLastError;
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_JOB_MEMORY,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectExtendedLimitInformation, SetInformationJobObject,
        };

        let handle = unsafe { CreateJobObjectW(null(), null()) };
        if handle.is_null() {
            let err = unsafe { GetLastError() };
            return Err(OpenCodeError::JobObject(format!(
                "CreateJobObjectW failed with Win32 error code {err}"
            )));
        }

        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        let mut limit_flags = 0u32;

        if policy.kill_on_close {
            limit_flags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        }

        if let Some(max_mem) = policy.max_memory_bytes {
            limit_flags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
            info.JobMemoryLimit = max_mem as usize;
        }

        if let Some(max_procs) = policy.max_processes {
            limit_flags |= JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
            info.BasicLimitInformation.ActiveProcessLimit = max_procs;
        }

        info.BasicLimitInformation.LimitFlags = limit_flags;

        let success = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const std::ffi::c_void,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };

        if success == 0 {
            let err = unsafe { GetLastError() };
            unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
            return Err(OpenCodeError::JobObject(format!(
                "SetInformationJobObject failed with Win32 error code {err}"
            )));
        }

        Ok(Self { handle })
    }

    pub fn raw_handle(&self) -> isize {
        self.handle as isize
    }

    pub fn assign_pid(&self, pid: u32) -> Result<(), OpenCodeError> {
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
        };

        let desired_access = PROCESS_SET_QUOTA | PROCESS_TERMINATE;
        let process_handle = unsafe { OpenProcess(desired_access, 0, pid) };
        if process_handle.is_null() {
            let err = unsafe { GetLastError() };
            return Err(OpenCodeError::JobObject(format!(
                "OpenProcess for PID {pid} failed with Win32 error code {err}"
            )));
        }

        let assigned = unsafe { AssignProcessToJobObject(self.handle, process_handle) };
        let assign_err = if assigned == 0 {
            unsafe { GetLastError() }
        } else {
            0
        };

        unsafe { CloseHandle(process_handle) };

        if assigned == 0 {
            return Err(OpenCodeError::JobObject(format!(
                "AssignProcessToJobObject for PID {pid} failed with Win32 error code {assign_err}"
            )));
        }

        Ok(())
    }

    pub fn terminate(&self, exit_code: u32) -> Result<(), OpenCodeError> {
        use windows_sys::Win32::Foundation::GetLastError;
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;

        let result = unsafe { TerminateJobObject(self.handle, exit_code) };
        if result == 0 {
            let err = unsafe { GetLastError() };
            return Err(OpenCodeError::JobObject(format!(
                "TerminateJobObject failed with Win32 error code {err}"
            )));
        }
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for JobObjectGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}
