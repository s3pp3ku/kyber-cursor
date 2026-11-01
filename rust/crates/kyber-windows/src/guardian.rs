//! A separate process survives a crash/forced termination of the desktop app.
use crate::{cursor, failed, wide};
use std::{
    process::{Child, Command},
    ptr::null,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{Foundation::*, System::Threading::*};

pub struct Handle(pub HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub struct Guardian {
    disarm: Handle,
    _ready: Handle,
    child: Child,
    finished: bool,
}
impl Guardian {
    pub fn start() -> Result<Self, String> {
        let pid = unsafe { GetCurrentProcessId() };
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let prefix = format!("Local\\KyberCursor-{pid}-{nonce}");
        let make = |name: &str| {
            let handle = unsafe { CreateEventW(null(), 1, 0, wide(name).as_ptr()) };
            if handle.is_null() {
                Err(failed("Create cursor recovery event"))
            } else {
                Ok(Handle(handle))
            }
        };
        let disarm = make(&format!("{prefix}-done"))?;
        let ready = make(&format!("{prefix}-ready"))?;
        let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--watch-parent")
            .arg(pid.to_string())
            .arg(&prefix)
            .spawn()
            .map_err(|e| e.to_string())?;
        if unsafe { WaitForSingleObject(ready.0, 10_000) } != WAIT_OBJECT_0 {
            unsafe {
                SetEvent(disarm.0);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(
                "Cursor recovery process did not start. Your pointer was left unchanged.".into(),
            );
        }
        Ok(Self {
            disarm,
            _ready: ready,
            child,
            finished: false,
        })
    }
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
    pub fn finish(&mut self) {
        unsafe {
            SetEvent(self.disarm.0);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        self.finished = true;
    }
}
impl Drop for Guardian {
    fn drop(&mut self) {
        if !self.finished {
            // The parent remains alive during stack unwinding. Restore first,
            // disarm only after that succeeds; otherwise the child recovers on exit.
            if cursor::restore_scheme().is_ok() {
                self.finish();
            }
        }
    }
}

pub fn watch() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    let pid = args
        .get(2)
        .and_then(|a| a.parse::<u32>().ok())
        .ok_or("Missing parent process")?;
    let prefix = args
        .get(3)
        .filter(|s| s.starts_with(&format!("Local\\KyberCursor-{pid}-")))
        .ok_or("Invalid recovery event")?;
    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if process.is_null() {
        return cursor::restore_scheme();
    }
    let process = Handle(process);
    let open = |suffix, access| {
        let handle = unsafe { OpenEventW(access, 0, wide(&format!("{prefix}-{suffix}")).as_ptr()) };
        if handle.is_null() {
            Err(failed("Open cursor recovery event"))
        } else {
            Ok(Handle(handle))
        }
    };
    let disarm = open("done", SYNCHRONIZATION_SYNCHRONIZE)?;
    let ready = open("ready", EVENT_MODIFY_STATE)?;
    if unsafe { SetEvent(ready.0) } == 0 {
        return Err(failed("Start cursor recovery"));
    }
    // Disarm wins if both objects are signaled following a clean shutdown.
    let handles = [disarm.0, process.0];
    let result = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) };
    if result == WAIT_OBJECT_0 {
        Ok(())
    } else {
        cursor::restore_scheme()
    }
}
