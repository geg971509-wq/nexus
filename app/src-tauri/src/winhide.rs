//! Hide console windows when spawning helper processes on Windows.

use std::process::Command;

/// `CREATE_NO_WINDOW` — no console flash for GUI-hosted child processes.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[inline]
#[allow(dead_code)] // used only on Windows spawn sites; mac keeps the no-op stub
pub fn apply(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}
