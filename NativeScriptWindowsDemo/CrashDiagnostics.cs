using System;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using Windows.Storage;

namespace NativeScriptWindowsDemo
{
    internal static class CrashDiagnostics
    {
        private static bool _installed;

        public static void InstallGlobalHandlers()
        {
            if (_installed)
            {
                return;
            }

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

            if (!string.IsNullOrWhiteSpace(details))
            {
                sb.AppendLine("Details: " + details);
            }

            if (ex != null)
            {
                sb.AppendLine("Exception:");
                sb.AppendLine(ex.ToString());
            }
            else
            {
                sb.AppendLine("Exception: <null>");
            }

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

        private static void AppendToLog(string content)
        {
            try
            {
                var localPath = ApplicationData.Current.LocalFolder.Path;
                var logPath = Path.Combine(localPath, "nativescript-crash.log");
                File.AppendAllText(logPath, content, Encoding.UTF8);
            }
            catch
            {
                // Diagnostics must never crash the app.
            }
        }
    }
}
