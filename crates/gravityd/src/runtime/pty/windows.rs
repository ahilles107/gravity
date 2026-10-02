//! Own the child handle so termination does not depend on portable-pty 0.8's
//! inverted WinChildKiller return-value check.
use std::io;
use std::os::windows::io::{AsRawHandle, BorrowedHandle, OwnedHandle, RawHandle};

#[link(name = "kernel32")]
extern "system" {
    fn GetExitCodeProcess(process: RawHandle, code: *mut u32) -> i32;
    fn TerminateProcess(process: RawHandle, code: u32) -> i32;
}

pub(super) struct ProcessKiller(OwnedHandle);

impl ProcessKiller {
    pub fn new(child: &dyn portable_pty::Child) -> anyhow::Result<Self> {
        let raw = child
            .as_raw_handle()
            .ok_or_else(|| anyhow::anyhow!("PTY child has no process handle"))?;
        // SAFETY: the child owns this valid process handle for this entire call.
        // Duplicate it before the child moves to its independent waiter thread.
        let borrowed = unsafe { BorrowedHandle::borrow_raw(raw) };
        Ok(Self(borrowed.try_clone_to_owned()?))
    }

    fn exited(&self) -> io::Result<bool> {
        let mut code = 0;
        // SAFETY: our owned process handle is live; code is a valid output pointer.
        if unsafe { GetExitCodeProcess(self.0.as_raw_handle(), &mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(code != 259) // STILL_ACTIVE
    }

    pub fn kill(&mut self) -> io::Result<()> {
        if self.exited()? {
            return Ok(());
        }
        // SAFETY: the duplicated handle refers only to this session's child.
        if unsafe { TerminateProcess(self.0.as_raw_handle(), 1) } != 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        // The child can finish between the status check and termination.
        if self.exited()? {
            Ok(())
        } else {
            Err(error)
        }
    }
}
