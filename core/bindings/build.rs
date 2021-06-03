fn main() {
    windows::build!(
        Windows::Win32::System::Console::WriteConsoleA,
        Windows::Win32::System::WindowsProgramming::STD_OUTPUT_HANDLE,
        Windows::Win32::System::WindowsProgramming::GetStdHandle,
        Windows::Win32::System::SystemServices::GetProcAddress,
        Windows::Win32::System::SystemServices::LoadLibraryA,
        Windows::Win32::System::SystemServices::FARPROC
    );
}