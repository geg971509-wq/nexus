//! Hide console windows when spawning helper processes on Windows.
//!
//! All Windows `Command` spawns that can be console apps (tasklist/taskkill/
//! curl/cscript/NexusCore CUI builds) MUST call [`apply`] before spawn/output.

use std::process::Command;

/// `CREATE_NO_WINDOW` — child gets no new console (no black flash).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[inline]
#[allow(dead_code)] // used only on Windows spawn sites; mac keeps the no-op stub
pub fn apply(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Do not combine DETACHED_PROCESS — breaks redirected stdout/stderr pipes.
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}
