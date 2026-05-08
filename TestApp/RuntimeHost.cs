using System;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Runtime.InteropServices;

namespace TestApp
{
    internal sealed class RuntimeHost : IDisposable
    {
        private const string NativeScriptLibrary = "nativescript";

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool AttachConsole(int dwProcessId);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool AllocConsole();

        private const int ATTACH_PARENT_PROCESS = -1;

        [DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_init))]
        private static extern long runtime_init([MarshalAs(UnmanagedType.LPUTF8Str)] string appRoot);

        [DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_deinit))]
        private static extern void runtime_deinit(long runtime);

        [DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_runscript))]
        private static extern void runtime_runscript(long runtime, [MarshalAs(UnmanagedType.LPUTF8Str)] string script);

        [DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_install_ctrlc_handler))]
        private static extern void runtime_install_ctrlc_handler(int exitCode);

        private long _runtime;
        private bool _initialized;

        private static void LogHost(string message)
        {
            Debug.WriteLine("[RuntimeHost] " + message);
            try
            {
                Console.WriteLine("[RuntimeHost] " + message);
            }
            catch
            {
                // No attached console stream, debug output still receives logs.
            }
        }

        private static void EnsureDiagnosticsConsole()
        {
            var attached = AttachConsole(ATTACH_PARENT_PROCESS);
            if (!attached)
            {
                var allocated = AllocConsole();
                if (!allocated)
                {
                    var error = Marshal.GetLastWin32Error();
                    LogHost("No console attached (Attach/Alloc failed, Win32=" + error + ").");
                    return;
                }
            }

            try
            {
                var stdout = Console.OpenStandardOutput();
                var writer = new StreamWriter(stdout, new UTF8Encoding(false)) { AutoFlush = true };
                Console.SetOut(writer);
                Console.SetError(writer);
            }
            catch (Exception ex)
            {
                Debug.WriteLine("[RuntimeHost] Failed to rebind console streams: " + ex.Message);
            }
        }

        public void Initialize()
        {
            if (_initialized)
            {
                return;
            }

            EnsureDiagnosticsConsole();

            runtime_install_ctrlc_handler(0);
            _runtime = runtime_init(AppContext.BaseDirectory);
            _initialized = true;
        }

        public void RunMainScript()
        {
            if (!_initialized)
            {
                throw new InvalidOperationException("Runtime must be initialized before running scripts.");
            }

            var entryPath = Path.Combine(AppContext.BaseDirectory, "App", "main.js");
            var script = File.ReadAllText(Path.GetFullPath(entryPath));
            runtime_runscript(_runtime, script);
        }

        public void Dispose()
        {
            if (!_initialized)
            {
                return;
            }

            runtime_deinit(_runtime);
            _runtime = 0;
            _initialized = false;
        }
    }
}
