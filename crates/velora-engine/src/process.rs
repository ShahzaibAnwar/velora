//! Running helper programs (yt-dlp, ffmpeg) without a console window, as a tree that can be stopped
//! as a whole.
//!
//! yt-dlp's Windows build is a self-extracting launcher that starts a second process, which in turn
//! starts ffmpeg. Killing only the first one would leave the others running, so on Windows every
//! helper is placed in a Job Object that terminates all of its processes when the job is closed.

use std::ffi::OsStr;
use std::process::Stdio;

use tokio::process::{Child, Command};

/// Owns a running helper. Dropping it ends the helper and everything it started.
pub struct ToolProcess {
    pub child: Child,
    #[cfg(windows)]
    _job: Option<job::Job>,
}

impl ToolProcess {
    pub fn spawn<I, S>(program: &std::path::Path, args: I) -> std::io::Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut cmd = Command::new(program);
        cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        // Run from the helper's own folder, never from a folder holding downloaded files.
        if let Some(dir) = program.parent().filter(|d| d.is_dir()) {
            cmd.current_dir(dir);
        }
        // yt-dlp is Python: make it print UTF-8 whatever the console code page is.
        cmd.env("PYTHONIOENCODING", "utf-8").env("PYTHONUTF8", "1");
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let child = cmd.spawn()?;
        #[cfg(windows)]
        let job = child
            .raw_handle()
            .and_then(|h| job::Job::assign(h).map_err(|e| tracing::warn!(error = %e, "couldn't create a job object")).ok());
        Ok(Self {
            child,
            #[cfg(windows)]
            _job: job,
        })
    }

    /// Stop the helper and all processes it started.
    pub async fn kill(mut self) {
        #[cfg(windows)]
        drop(self._job.take());
        let _ = self.child.kill().await;
    }
}

#[cfg(windows)]
mod job {
    use std::os::windows::io::RawHandle;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    pub struct Job(HANDLE);

    // The handle is only used to close the job.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Job {
        pub fn assign(process: RawHandle) -> std::io::Result<Self> {
            // SAFETY: plain Win32 calls on handles we own; failures are checked.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    return Err(std::io::Error::last_os_error());
                }
                let job = Job(job);
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if AssignProcessToJobObject(job.0, process as HANDLE) == 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(job)
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: the handle came from CreateJobObjectW and is closed once.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}
