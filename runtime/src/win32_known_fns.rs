/// Compile-time registry of well-known Win32 function signatures.
///
/// Every entry here gets its CIF and function pointer pre-built at `Runtime::new`
/// so the first JS call to that function pays no `GetProcAddress` or `Cif::new`
/// cost.  It also lets `__nsWin32CallRaw` skip the per-call type-string parsing
/// for registered functions.
///
/// # Adding a new entry
///
/// 1. Find the entry for the correct DLL group (or add a new group comment).
/// 2. Append a line:
///    ```
///    KnownFn { dll: "somedll.dll", name: "FunctionName", ret: "retType", params: &["arg0type", …] },
///    ```
/// 3. Rebuild.  No other files need to change.
///
/// Valid type strings: "void" "bool" "i8" "i16" "i32" "i64" "u8" "u16" "u32" "u64"
///                     "f32" "f64" "pointer" "wstr" "str"

/// One registered Win32 function.
pub struct KnownFn {
    pub dll:    &'static str,
    pub name:   &'static str,
    /// libffi return type tag (see type strings above).
    pub ret:    &'static str,
    /// libffi parameter type tags, in call order.
    pub params: &'static [&'static str],
}

pub static KNOWN_FNS: &[KnownFn] = &[
    // ── kernel32.dll ─────────────────────────────────────────────────────────
    KnownFn { dll: "kernel32.dll", name: "GetTickCount64",           ret: "u64",     params: &[] },
    KnownFn { dll: "kernel32.dll", name: "GetTickCount",             ret: "u32",     params: &[] },
    KnownFn { dll: "kernel32.dll", name: "Sleep",                    ret: "void",    params: &["u32"] },
    KnownFn { dll: "kernel32.dll", name: "SleepEx",                  ret: "u32",     params: &["u32", "bool"] },
    KnownFn { dll: "kernel32.dll", name: "GetCurrentThreadId",       ret: "u32",     params: &[] },
    KnownFn { dll: "kernel32.dll", name: "GetCurrentProcessId",      ret: "u32",     params: &[] },
    KnownFn { dll: "kernel32.dll", name: "ExitProcess",              ret: "void",    params: &["u32"] },
    KnownFn { dll: "kernel32.dll", name: "GetLastError",             ret: "u32",     params: &[] },
    KnownFn { dll: "kernel32.dll", name: "SetLastError",             ret: "void",    params: &["u32"] },
    KnownFn { dll: "kernel32.dll", name: "QueryPerformanceCounter",  ret: "bool",    params: &["pointer"] },
    KnownFn { dll: "kernel32.dll", name: "QueryPerformanceFrequency",ret: "bool",    params: &["pointer"] },
    KnownFn { dll: "kernel32.dll", name: "GetSystemTimeAsFileTime",  ret: "void",    params: &["pointer"] },
    KnownFn { dll: "kernel32.dll", name: "GetModuleHandleW",         ret: "pointer", params: &["pointer"] },
    KnownFn { dll: "kernel32.dll", name: "GetProcAddress",           ret: "pointer", params: &["pointer", "str"] },
    KnownFn { dll: "kernel32.dll", name: "VirtualAlloc",             ret: "pointer", params: &["pointer", "u64", "u32", "u32"] },
    KnownFn { dll: "kernel32.dll", name: "VirtualFree",              ret: "bool",    params: &["pointer", "u64", "u32"] },
    KnownFn { dll: "kernel32.dll", name: "CreateEventW",             ret: "pointer", params: &["pointer", "bool", "bool", "pointer"] },
    KnownFn { dll: "kernel32.dll", name: "SetEvent",                 ret: "bool",    params: &["pointer"] },
    KnownFn { dll: "kernel32.dll", name: "ResetEvent",               ret: "bool",    params: &["pointer"] },
    KnownFn { dll: "kernel32.dll", name: "WaitForSingleObject",      ret: "u32",     params: &["pointer", "u32"] },
    KnownFn { dll: "kernel32.dll", name: "CloseHandle",              ret: "bool",    params: &["pointer"] },
    KnownFn { dll: "kernel32.dll", name: "GetConsoleWindow",         ret: "pointer", params: &[] },
    KnownFn { dll: "kernel32.dll", name: "AllocConsole",             ret: "bool",    params: &[] },
    KnownFn { dll: "kernel32.dll", name: "FreeConsole",              ret: "bool",    params: &[] },
    KnownFn { dll: "kernel32.dll", name: "OutputDebugStringW",       ret: "void",    params: &["wstr"] },

    // ── user32.dll ────────────────────────────────────────────────────────────
    KnownFn { dll: "user32.dll", name: "GetSystemMetrics",           ret: "i32",     params: &["i32"] },
    KnownFn { dll: "user32.dll", name: "MessageBoxW",                ret: "i32",     params: &["pointer", "wstr", "wstr", "u32"] },
    KnownFn { dll: "user32.dll", name: "MessageBeep",                ret: "bool",    params: &["u32"] },
    KnownFn { dll: "user32.dll", name: "FindWindowW",                ret: "pointer", params: &["pointer", "pointer"] },
    KnownFn { dll: "user32.dll", name: "GetForegroundWindow",        ret: "pointer", params: &[] },
    KnownFn { dll: "user32.dll", name: "SetForegroundWindow",        ret: "bool",    params: &["pointer"] },
    KnownFn { dll: "user32.dll", name: "ShowWindow",                 ret: "bool",    params: &["pointer", "i32"] },
    KnownFn { dll: "user32.dll", name: "SetWindowTextW",             ret: "bool",    params: &["pointer", "wstr"] },
    KnownFn { dll: "user32.dll", name: "GetWindowTextLengthW",       ret: "i32",     params: &["pointer"] },
    KnownFn { dll: "user32.dll", name: "PostMessageW",               ret: "bool",    params: &["pointer", "u32", "u64", "i64"] },
    KnownFn { dll: "user32.dll", name: "SendMessageW",               ret: "i64",     params: &["pointer", "u32", "u64", "i64"] },
    KnownFn { dll: "user32.dll", name: "GetCursorPos",               ret: "bool",    params: &["pointer"] },
    KnownFn { dll: "user32.dll", name: "SetCursorPos",               ret: "bool",    params: &["i32", "i32"] },
    KnownFn { dll: "user32.dll", name: "GetKeyState",                ret: "i16",     params: &["i32"] },
    KnownFn { dll: "user32.dll", name: "GetAsyncKeyState",           ret: "i16",     params: &["i32"] },
    KnownFn { dll: "user32.dll", name: "keybd_event",                ret: "void",    params: &["u8", "u8", "u32", "u64"] },
    KnownFn { dll: "user32.dll", name: "mouse_event",                ret: "void",    params: &["u32", "i32", "i32", "u32", "u64"] },
    KnownFn { dll: "user32.dll", name: "ClipCursor",                 ret: "bool",    params: &["pointer"] },
    KnownFn { dll: "user32.dll", name: "GetDesktopWindow",           ret: "pointer", params: &[] },
    KnownFn { dll: "user32.dll", name: "GetClientRect",              ret: "bool",    params: &["pointer", "pointer"] },
    KnownFn { dll: "user32.dll", name: "GetWindowRect",              ret: "bool",    params: &["pointer", "pointer"] },
    KnownFn { dll: "user32.dll", name: "MoveWindow",                 ret: "bool",    params: &["pointer", "i32", "i32", "i32", "i32", "bool"] },
    KnownFn { dll: "user32.dll", name: "InvalidateRect",             ret: "bool",    params: &["pointer", "pointer", "bool"] },
    KnownFn { dll: "user32.dll", name: "UpdateWindow",               ret: "bool",    params: &["pointer"] },
    KnownFn { dll: "user32.dll", name: "DestroyWindow",              ret: "bool",    params: &["pointer"] },

    // ── ntdll.dll ─────────────────────────────────────────────────────────────
    KnownFn { dll: "ntdll.dll", name: "NtQuerySystemTime",           ret: "i32",     params: &["pointer"] },
    KnownFn { dll: "ntdll.dll", name: "RtlGetVersion",               ret: "i32",     params: &["pointer"] },
    KnownFn { dll: "ntdll.dll", name: "RtlMoveMemory",               ret: "void",    params: &["pointer", "pointer", "u64"] },

    // ── winmm.dll ─────────────────────────────────────────────────────────────
    KnownFn { dll: "winmm.dll", name: "timeGetTime",                 ret: "u32",     params: &[] },
    KnownFn { dll: "winmm.dll", name: "timeBeginPeriod",             ret: "u32",     params: &["u32"] },
    KnownFn { dll: "winmm.dll", name: "timeEndPeriod",               ret: "u32",     params: &["u32"] },
    KnownFn { dll: "winmm.dll", name: "mciSendStringW",              ret: "u32",     params: &["wstr", "pointer", "u32", "pointer"] },

    // ── gdi32.dll ─────────────────────────────────────────────────────────────
    KnownFn { dll: "gdi32.dll", name: "GetDeviceCaps",               ret: "i32",     params: &["pointer", "i32"] },
    KnownFn { dll: "gdi32.dll", name: "CreateSolidBrush",            ret: "pointer", params: &["u32"] },
    KnownFn { dll: "gdi32.dll", name: "DeleteObject",                ret: "bool",    params: &["pointer"] },

    // ── shell32.dll ───────────────────────────────────────────────────────────
    KnownFn { dll: "shell32.dll", name: "ShellExecuteW",             ret: "pointer", params: &["pointer", "wstr", "wstr", "wstr", "wstr", "i32"] },
    KnownFn { dll: "shell32.dll", name: "SHGetFolderPathW",          ret: "i32",     params: &["pointer", "i32", "pointer", "u32", "pointer"] },

    // ── advapi32.dll ──────────────────────────────────────────────────────────
    KnownFn { dll: "advapi32.dll", name: "RegOpenKeyExW",            ret: "i32",     params: &["pointer", "wstr", "u32", "u32", "pointer"] },
    KnownFn { dll: "advapi32.dll", name: "RegQueryValueExW",         ret: "i32",     params: &["pointer", "wstr", "pointer", "pointer", "pointer", "pointer"] },
    KnownFn { dll: "advapi32.dll", name: "RegCloseKey",              ret: "i32",     params: &["pointer"] },
    KnownFn { dll: "advapi32.dll", name: "GetUserNameW",             ret: "bool",    params: &["pointer", "pointer"] },

    // ── psapi.dll / kernel32 (Vista+) ─────────────────────────────────────────
    KnownFn { dll: "psapi.dll",    name: "GetProcessMemoryInfo",     ret: "bool",    params: &["pointer", "pointer", "u32"] },
    KnownFn { dll: "kernel32.dll", name: "K32GetProcessMemoryInfo",  ret: "bool",    params: &["pointer", "pointer", "u32"] },
];

/// Look up a function by DLL name (case-insensitive) and exact function name.
/// Returns `None` if the function is not in the registry.
pub fn lookup(dll: &str, name: &str) -> Option<&'static KnownFn> {
    KNOWN_FNS.iter().find(|f| {
        f.name == name && f.dll.eq_ignore_ascii_case(dll)
    })
}
