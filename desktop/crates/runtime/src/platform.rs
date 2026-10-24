//! OS locks and owned worker trees for the isolated feasibility probe.
use anyhow::{Context, Result, ensure};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
    process::{Child, Command},
};

pub struct FileLock {
    _file: File,
}
impl FileLock {
    pub fn acquire(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)?;
        // Match Python's nonempty file, offset zero, one-byte lock. Never truncate.
        if file.metadata()?.len() == 0 {
            file.write_all(b" ")?;
            file.flush()?;
        }
        lock(&file).context("notebook lock is already held or unavailable")?;
        Ok(Self { _file: file })
    }
}

#[cfg(windows)]
fn lock(file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Storage::FileSystem::{LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx},
        System::IO::OVERLAPPED,
    };
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        LockFileEx(
            file.as_raw_handle(),
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            1,
            0,
            &mut overlapped,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
#[cfg(unix)]
fn lock(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub struct WorkerTree {
    pub child: Child,
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
}
impl WorkerTree {
    pub fn spawn(command: &mut Command) -> Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x00000008 | 0x00000200 | 0x08000000);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[allow(unused_mut)]
        let mut child = command.spawn().context("worker launch failed")?;
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
            let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = !job.is_null()
                && unsafe {
                    SetInformationJobObject(
                        job,
                        JobObjectExtendedLimitInformation,
                        &limits as *const _ as *const _,
                        std::mem::size_of_val(&limits) as u32,
                    ) != 0
                        && AssignProcessToJobObject(job, child.as_raw_handle()) != 0
                };
            if !ok {
                let error = io::Error::last_os_error();
                let _ = child.kill();
                let _ = child.wait();
                if !job.is_null() {
                    unsafe {
                        CloseHandle(job);
                    }
                }
                return Err(error).context("cannot contain worker in Job Object");
            }
            Ok(Self { child, job })
        }
        #[cfg(unix)]
        {
            Ok(Self { child })
        }
    }
    pub fn terminate(&mut self) -> Result<()> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;
            ensure!(
                unsafe { TerminateJobObject(self.job, 1) } != 0,
                "cannot terminate worker job: {}",
                io::Error::last_os_error()
            );
        }
        #[cfg(unix)]
        {
            let outcome = unsafe { libc::kill(-(self.child.id() as i32), libc::SIGKILL) };
            ensure!(
                outcome == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH),
                "cannot terminate worker group"
            );
        }
        let _ = self.child.kill();
        self.child.wait()?;
        Ok(())
    }
}
impl Drop for WorkerTree {
    fn drop(&mut self) {
        let _ = self.terminate();
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}

/// Match the Python supervisor's Windows venv redirector avoidance.
pub fn python_command(python: &Path) -> Result<Command> {
    ensure!(
        python.is_absolute() && python.is_file(),
        "interpreter must be an existing absolute path"
    );
    #[cfg(windows)]
    {
        let parent = python.parent().context("interpreter has no parent")?;
        let candidates = [
            parent.parent().unwrap_or(parent).join("pyvenv.cfg"),
            parent.join("pyvenv.cfg"),
        ];
        if let Some(cfg) = candidates.iter().find(|path| path.is_file()) {
            let contents = std::fs::read_to_string(cfg)?;
            let mut values = std::collections::BTreeMap::new();
            for line in contents.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    values.insert(
                        key.trim().to_lowercase(),
                        value.trim().trim_matches('"').to_owned(),
                    );
                }
            }
            let executable = values
                .get("home")
                .map(|home| Path::new(home).join("python.exe"))
                .filter(|path| path.is_file())
                .or_else(|| values.get("executable").map(std::path::PathBuf::from));
            if let Some(base) = executable.filter(|path| path.is_file() && path != python) {
                let mut command = Command::new(base);
                command.env("__PYVENV_LAUNCHER__", python);
                return Ok(command);
            }
        }
    }
    Ok(Command::new(python))
}
