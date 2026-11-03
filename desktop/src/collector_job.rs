//! A suspended child cannot execute before the desktop owns its lifetime.
use std::{
    io,
    mem::size_of,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    process::Child,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::*,
        JobObjects::*,
        Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
    },
};
pub struct Job(#[allow(dead_code)] OwnedHandle);
impl Job {
    pub fn attach(child: &Child) -> io::Result<Self> {
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Self(OwnedHandle::from_raw_handle(handle));
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle()) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let _snapshot = OwnedHandle::from_raw_handle(snapshot);
            let mut entry = THREADENTRY32 {
                dwSize: size_of::<THREADENTRY32>() as u32,
                ..Default::default()
            };
            let mut next = Thread32First(snapshot, &mut entry);
            while next != 0 {
                if entry.th32OwnerProcessID == child.id() {
                    let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                    if thread.is_null() {
                        return Err(io::Error::last_os_error());
                    }
                    let resumed = ResumeThread(thread);
                    CloseHandle(thread);
                    if resumed == u32::MAX {
                        return Err(io::Error::last_os_error());
                    }
                    return Ok(job);
                }
                next = Thread32Next(snapshot, &mut entry);
            }
            Err(io::Error::other("Cannot find suspended collector thread"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::{
        fs,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };
    #[test]
    fn parent_helper() {
        let Some(path) = std::env::var_os("NAUCTIONS_JOB_TEST_READY") else {
            return;
        };
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "collector::tests::child_helper"])
            .env("NAUCTIONS_TEST_CHILD", "1")
            .creation_flags(0x08000004)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let _job = Job::attach(&child).unwrap();
        fs::write(path, child.id().to_string()).unwrap();
        let _ = child.wait();
    }
    #[test]
    fn parent_termination_kills_collector_before_restart() {
        let dir = tempfile::tempdir().unwrap();
        let ready = dir.path().join("ready");
        let mut parent = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "collector_job::tests::parent_helper"])
            .env("NAUCTIONS_JOB_TEST_READY", &ready)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        let pid: u32 = loop {
            if let Some(pid) = fs::read_to_string(&ready)
                .ok()
                .and_then(|contents| contents.parse().ok())
            {
                break pid;
            }
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "helper PID was not ready"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        unsafe {
            let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            assert!(!process.is_null());
            let _process = OwnedHandle::from_raw_handle(process);
            assert_eq!(WaitForSingleObject(process, 0), 258); // WAIT_TIMEOUT
            parent.kill().unwrap();
            parent.wait().unwrap();
            assert_eq!(WaitForSingleObject(process, 5000), 0); // WAIT_OBJECT_0
        }
    }
}
