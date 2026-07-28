//! Last-chance native crash reporter. Windows calls the top-level unhandled-exception filter on
//! the *faulting* thread with its stack still intact, so `backtrace::Backtrace::new()` from inside
//! the filter captures the frames that led to the fault, symbolized via the host's PDB (release
//! profile `debug = true`). Same reporter used to root-cause the QuickJS host.

use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS;

// This windows crate version doesn't generate SetUnhandledExceptionFilter; declare it directly.
type TopLevelFilter = Option<unsafe extern "system" fn(*const EXCEPTION_POINTERS) -> i32>;
#[link(name = "kernel32")]
extern "system" {
    fn SetUnhandledExceptionFilter(f: TopLevelFilter) -> TopLevelFilter;
}

static IN_HANDLER: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn handler(info: *const EXCEPTION_POINTERS) -> i32 {
    if IN_HANDLER.swap(true, Ordering::SeqCst) {
        return 1; // EXCEPTION_EXECUTE_HANDLER → terminate
    }
    let (code, addr, access, data_addr) = if !info.is_null() {
        let rec = (*info).ExceptionRecord;
        if !rec.is_null() {
            let (acc, da) = if (*rec).NumberParameters >= 2 {
                ((*rec).ExceptionInformation[0], (*rec).ExceptionInformation[1])
            } else {
                (usize::MAX, 0)
            };
            ((*rec).ExceptionCode.0 as u32, (*rec).ExceptionAddress as usize, acc, da)
        } else {
            (0, 0, usize::MAX, 0)
        }
    } else {
        (0, 0, usize::MAX, 0)
    };
    let kind = match access {
        0 => "READ",
        1 => "WRITE",
        8 => "DEP/EXEC",
        _ => "?",
    };
    eprintln!(
        "\n=== NATIVE CRASH: code=0x{code:08X} instr=0x{addr:X} {kind} of data_addr=0x{data_addr:X} ==="
    );
    let bt = backtrace::Backtrace::new();
    eprintln!("{bt:?}");
    use std::io::Write;
    std::io::stderr().flush().ok();
    1
}

/// Install the top-level filter. Call once, early in `main`.
pub fn install() {
    unsafe {
        SetUnhandledExceptionFilter(Some(handler));
    }
}
