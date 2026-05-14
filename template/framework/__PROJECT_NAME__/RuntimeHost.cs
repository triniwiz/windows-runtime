using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text.Json;

namespace __PROJECT_NAME__
{
    internal sealed class RuntimeHost : IDisposable
    {
        private const string NativeScriptLibrary = "nativescript";

        [DllImport("kernel32.dll")]
        private static extern bool AttachConsole(int dwProcessId);

        private const int ATTACH_PARENT_PROCESS = -1;

        [DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_init))]
        private static extern long runtime_init([MarshalAs(UnmanagedType.LPUTF8Str)] string appRoot);

        [DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_deinit))]
        private static extern void runtime_deinit(long runtime);

        [DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_runscript))]
        private static extern void runtime_runscript(long runtime, [MarshalAs(UnmanagedType.LPUTF8Str)] string script, [MarshalAs(UnmanagedType.LPUTF8Str)] string filename);

        [DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_install_ctrlc_handler))]
        private static extern void runtime_install_ctrlc_handler(int exitCode);

#if DEBUG
        [DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_devtools_start))]
        private static extern IntPtr runtime_devtools_start(long runtime, ushort port);

        [DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_devtools_pump))]
        private static extern void runtime_devtools_pump(long runtime);

        [DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_free_string))]
        private static extern void runtime_free_string(IntPtr ptr);

        public string DevtoolsFrontendUrl { get; private set; }

        public void PumpDevtools()
        {
            if (!_initialized) return;
            try { runtime_devtools_pump(_runtime); }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"[NativeScript DevTools] Pump failed: {ex.Message}");
            }
        }

        private void StartDevtoolsSafely()
        {
            IntPtr urlPtr = IntPtr.Zero;
            try
            {
                urlPtr = runtime_devtools_start(_runtime, 42000);
                if (urlPtr == IntPtr.Zero) return;
                var wsUrl = Marshal.PtrToStringUTF8(urlPtr);
                DevtoolsFrontendUrl = wsUrl != null
                    ? $"devtools://devtools/bundled/inspector.html?ws={wsUrl.Replace("ws://", "")}"
                    : null;
                if (DevtoolsFrontendUrl != null)
                    System.Diagnostics.Debug.WriteLine($"[NativeScript DevTools] {DevtoolsFrontendUrl}");
            }
            catch (Exception ex)
            {
                DevtoolsFrontendUrl = null;
                System.Diagnostics.Debug.WriteLine($"[NativeScript DevTools] Start failed: {ex.Message}");
            }
            finally
            {
                if (urlPtr != IntPtr.Zero) runtime_free_string(urlPtr);
            }
        }
#endif

        private long _runtime;
        private bool _initialized;

        public void Initialize()
        {
            if (_initialized) return;
            AttachConsole(ATTACH_PARENT_PROCESS);
            runtime_install_ctrlc_handler(0);
            _runtime = runtime_init(AppContext.BaseDirectory);
#if DEBUG
            if (ConsumeDebugBreakMarker())
                StartDevtoolsSafely();
#endif
            _initialized = true;
        }

#if DEBUG
        /// <summary>
        /// Returns true and deletes the marker if the CLI wrote ns-debugbreak to LocalFolder,
        /// matching the Android sentinel-file pattern used by the NativeScript CLI.
        /// </summary>
        private static bool ConsumeDebugBreakMarker()
        {
            try
            {
                var markerPath = System.IO.Path.Combine(
                    Windows.Storage.ApplicationData.Current.LocalFolder.Path,
                    "ns-debugbreak");
                if (!System.IO.File.Exists(markerPath))
                    return false;
                System.IO.File.Delete(markerPath);
                return true;
            }
            catch { return false; }
        }
#endif

        public void RunMainScript()
        {
            if (!_initialized)
                throw new InvalidOperationException("Runtime must be initialized before running scripts.");

            var entryPath = ResolveEntryScriptPath();
            var script = File.ReadAllText(Path.GetFullPath(entryPath));
            try
            {
                runtime_runscript(_runtime, script, Path.GetFileName(entryPath));
            }
            catch (Exception ex)
            {
                CrashDiagnostics.WriteExceptionReport("RuntimeHost.RunMainScript", ex, "EntryScript=" + entryPath);
                System.Diagnostics.Debug.WriteLine($"[NativeScript Runtime] Script execution failed ({entryPath}): {ex}");
                throw;
            }
        }

        private sealed class RuntimePackageConfig
        {
            public string Main { get; set; } = string.Empty;
            public string WindowsMain { get; set; } = string.Empty;
        }

        private static string ResolveEntryScriptPath()
        {
            var baseDir = AppContext.BaseDirectory;
            // EXE lives in <project>/bin/; webpack bundle lives in <project>/app/.
            var parentDir = Path.GetDirectoryName(
                baseDir.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar))
                ?? baseDir;

            // Candidate app directories: sibling of bin/ first, then directly under baseDir.
            var appDirCandidates = new[]
            {
                Path.Combine(parentDir, "app"),
                Path.Combine(parentDir, "App"),
                Path.Combine(baseDir, "app"),
                Path.Combine(baseDir, "App"),
            };

            string packageJsonPath = null;
            string resolvedBaseDir = null;
            foreach (var dir in appDirCandidates)
            {
                var candidate = Path.Combine(dir, "package.json");
                if (File.Exists(candidate))
                {
                    packageJsonPath = candidate;
                    resolvedBaseDir = dir;
                    break;
                }
            }

            // Also accept package.json at the project root (parent of bin/).
            if (packageJsonPath == null && File.Exists(Path.Combine(parentDir, "package.json")))
            {
                packageJsonPath = Path.Combine(parentDir, "package.json");
                resolvedBaseDir = parentDir;
            }

            string Fallback() =>
                appDirCandidates.Select(d => Path.Combine(d, "bundle.js")).FirstOrDefault(File.Exists);

            if (packageJsonPath == null)
                return Fallback();

            try
            {
                var config = ParsePackageConfig(packageJsonPath);
                if (!string.IsNullOrWhiteSpace(config.WindowsMain))
                {
                    var p = ResolveScriptPath(resolvedBaseDir, config.WindowsMain);
                    if (p != null) return p;
                }
                if (!string.IsNullOrWhiteSpace(config.Main))
                {
                    var p = ResolveScriptPath(resolvedBaseDir, config.Main);
                    if (p != null) return p;
                }
            }
            catch { }

            return Fallback();
        }

        private static RuntimePackageConfig ParsePackageConfig(string packageJsonPath)
        {
            using var doc = JsonDocument.Parse(File.ReadAllText(packageJsonPath));
            var config = new RuntimePackageConfig();
            if (doc.RootElement.TryGetProperty("main", out var main) && main.ValueKind == JsonValueKind.String)
                config.Main = main.GetString();
            if (doc.RootElement.TryGetProperty("windows", out var win) && win.ValueKind == JsonValueKind.Object &&
                win.TryGetProperty("main", out var winMain) && winMain.ValueKind == JsonValueKind.String)
                config.WindowsMain = winMain.GetString();
            return config;
        }

        private static string ResolveScriptPath(string baseDir, string scriptPath)
        {
            if (string.IsNullOrWhiteSpace(scriptPath)) return null;
            var normalized = scriptPath.Replace('/', Path.DirectorySeparatorChar);
            foreach (var candidate in new[] { normalized, normalized + ".js" })
            {
                var direct = Path.IsPathRooted(candidate) ? candidate : Path.Combine(baseDir, candidate);
                if (File.Exists(direct)) return direct;
                var appLower = Path.Combine(baseDir, "app", candidate);
                if (File.Exists(appLower)) return appLower;
                var appUpper = Path.Combine(baseDir, "App", candidate);
                if (File.Exists(appUpper)) return appUpper;
            }
            return null;
        }

        public void Dispose()
        {
            if (!_initialized) return;
            runtime_deinit(_runtime);
            _runtime = 0;
            _initialized = false;
        }
    }
}
