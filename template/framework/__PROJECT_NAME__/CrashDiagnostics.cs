using System;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using Windows.ApplicationModel.Core;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage;
using Windows.UI.Popups;

namespace __PROJECT_NAME__
{
    internal static class CrashDiagnostics
    {
        private static bool _installed;

        public static void InstallGlobalHandlers()
        {
            if (_installed) return;
            _installed = true;

            AppDomain.CurrentDomain.UnhandledException += (_, args) =>
            {
                var ex = args.ExceptionObject as Exception;
                WriteExceptionReport("AppDomain.UnhandledException", ex, "IsTerminating=" + args.IsTerminating);
            };

            TaskScheduler.UnobservedTaskException += (_, args) =>
            {
                WriteExceptionReport("TaskScheduler.UnobservedTaskException", args.Exception, null);
            };
        }

        public static void WriteExceptionReport(string source, Exception ex, string details)
        {
            var sb = new StringBuilder();
            sb.AppendLine("============================================================");
            sb.AppendLine("Timestamp: " + DateTimeOffset.UtcNow.ToString("o"));
            sb.AppendLine("Source: " + source);
            if (!string.IsNullOrWhiteSpace(details)) sb.AppendLine("Details: " + details);
            if (ex != null) { sb.AppendLine("Exception:"); sb.AppendLine(ex.ToString()); }
            else sb.AppendLine("Exception: <null>");
            sb.AppendLine("Managed stack snapshot:");
            sb.AppendLine(Environment.StackTrace);
            sb.AppendLine();
            AppendToLog(sb.ToString());
        }

        public static void WriteMessage(string source, string message)
        {
            var sb = new StringBuilder();
            sb.AppendLine("============================================================");
            sb.AppendLine("Timestamp: " + DateTimeOffset.UtcNow.ToString("o"));
            sb.AppendLine("Source: " + source);
            sb.AppendLine("Message: " + message);
            sb.AppendLine();
            AppendToLog(sb.ToString());
        }

        public static string BuildErrorReport(Exception ex, string jsError = null, string extraDetails = null)
        {
            var sb = new StringBuilder();

            if (!string.IsNullOrWhiteSpace(jsError))
            {
                sb.AppendLine("── JavaScript Error ──────────────────────────────────────");
                sb.AppendLine(jsError.Trim());
                sb.AppendLine();
            }

            if (ex != null)
            {
                sb.AppendLine("── Native Exception ──────────────────────────────────────");
                sb.AppendLine(ex.GetType().Name + ": " + ex.Message);
                if (ex.StackTrace != null) sb.AppendLine(ex.StackTrace);
                var inner = ex.InnerException;
                while (inner != null)
                {
                    sb.AppendLine("Caused by: " + inner.GetType().Name + ": " + inner.Message);
                    if (inner.StackTrace != null) sb.AppendLine(inner.StackTrace);
                    inner = inner.InnerException;
                }
                sb.AppendLine();
            }

            // Include panic log from previous or current run
            try
            {
                var panicLog = Path.Combine(ApplicationData.Current.LocalFolder.Path, "nativescript-panic.log");
                if (File.Exists(panicLog))
                {
                    var content = File.ReadAllText(panicLog, Encoding.UTF8).Trim();
                    if (!string.IsNullOrEmpty(content))
                    {
                        sb.AppendLine("── Rust Panic Log ────────────────────────────────────────");
                        sb.AppendLine(content);
                        sb.AppendLine();
                    }
                }
            }
            catch { }

            if (!string.IsNullOrWhiteSpace(extraDetails))
            {
                sb.AppendLine("── Additional Info ───────────────────────────────────────");
                sb.AppendLine(extraDetails);
                sb.AppendLine();
            }

            sb.AppendLine("──────────────────────────────────────────────────────────");
            sb.AppendLine("Timestamp: " + DateTimeOffset.UtcNow.ToString("o"));

            return sb.ToString();
        }

        public static string CrashLogPath()
        {
            try { return Path.Combine(ApplicationData.Current.LocalFolder.Path, "nativescript-crash.log"); }
            catch { return null; }
        }

        public static async Task ShowCrashDialogAsync(string heading, string errorReport)
        {
            try
            {
                // Write full details to log so developer can access them even if the
                // dialog summary is truncated.
                var logPath = CrashLogPath();
                if (logPath != null)
                    File.WriteAllText(logPath, errorReport, Encoding.UTF8);

                // Truncate for the dialog (MessageDialog has practical limits).
                const int MaxLen = 800;
                var summary = errorReport.Length > MaxLen
                    ? errorReport.Substring(0, MaxLen) + "\n\n[truncated — see log for full details]"
                    : errorReport;

                if (logPath != null)
                    summary += "\n\nFull log: " + logPath;

                var dialog = new MessageDialog(summary, "NativeScript Runtime Error");

                dialog.Commands.Add(new UICommand("Copy Details", _ =>
                {
                    try
                    {
                        var dp = new DataPackage();
                        dp.SetText(errorReport);
                        Clipboard.SetContent(dp);
                    }
                    catch { }
                }));

                dialog.Commands.Add(new UICommand("Restart App", async _ =>
                {
                    try { await CoreApplication.RequestRestartAsync("crash-restart"); }
                    catch { }
                }));

                dialog.Commands.Add(new UICommand("Dismiss"));
                dialog.DefaultCommandIndex = 0;
                dialog.CancelCommandIndex = 2;

                await dialog.ShowAsync();
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine("[NativeScript] Failed to show crash dialog: " + ex.Message);
            }
        }

        private static void AppendToLog(string content)
        {
            try
            {
                var logPath = Path.Combine(ApplicationData.Current.LocalFolder.Path, "nativescript-crash.log");
                File.AppendAllText(logPath, content, Encoding.UTF8);
            }
            catch { }
        }
    }
}
