using System;
using System.IO;
using System.Runtime.InteropServices;

namespace TestApp
{
    internal sealed class RuntimeHost : IDisposable
    {
        [DllImport("kernel32.dll")]
        private static extern bool AttachConsole(int dwProcessId);

        private const int ATTACH_PARENT_PROCESS = -1;

        [DllImport("libs\\nativescript.dll")]
        private static extern long runtime_init([MarshalAs(UnmanagedType.LPUTF8Str)] string appRoot);

        [DllImport("libs\\nativescript.dll")]
        private static extern void runtime_deinit(long runtime);

        [DllImport("libs\\nativescript.dll")]
        private static extern void runtime_runscript(long runtime, [MarshalAs(UnmanagedType.LPUTF8Str)] string script);

        [DllImport("libs\\nativescript.dll")]
        private static extern void runtime_install_ctrlc_handler(int exitCode);

        private long _runtime;
        private bool _initialized;

        public void Initialize()
        {
            if (_initialized)
            {
                return;
            }

            // Best effort for CLI-launched debug sessions.
            AttachConsole(ATTACH_PARENT_PROCESS);

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
