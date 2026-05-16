using System;
using Windows.ApplicationModel;
using Windows.ApplicationModel.Activation;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Navigation;

namespace __PROJECT_NAME__
{
    sealed partial class App : Application
    {
        private const string LastLaunchArgsKey = "LastLaunchArgs";
        private readonly RuntimeHost _runtimeHost = new RuntimeHost();

        public App()
        {
            CrashDiagnostics.InstallGlobalHandlers();
            this.Suspending += OnSuspending;
            this.UnhandledException += OnUnhandledException;
        }

        protected override async void OnLaunched(LaunchActivatedEventArgs e)
        {
            _runtimeHost.Initialize();

            // Show crash report from the previous run if one exists.
            try
            {
                var panicLogPath = System.IO.Path.Combine(
                    ApplicationData.Current.LocalFolder.Path, "nativescript-panic.log");
                if (System.IO.File.Exists(panicLogPath))
                {
                    var content = System.IO.File.ReadAllText(panicLogPath);
                    System.IO.File.Delete(panicLogPath);
                    if (!string.IsNullOrWhiteSpace(content))
                        await CrashDiagnostics.ShowCrashDialogAsync("Crash from previous run", content);
                }
            }
            catch { }

            string jsError = null;
            try
            {
                _runtimeHost.RunMainScript();
                jsError = _runtimeHost.GetLastJsError();
                if (!string.IsNullOrEmpty(jsError))
                {
                    CrashDiagnostics.WriteMessage("JS Error", jsError);
                    var report = CrashDiagnostics.BuildErrorReport(null, jsError);
                    await CrashDiagnostics.ShowCrashDialogAsync("JavaScript Error", report);
                }
            }
            catch (Exception scriptEx)
            {
                jsError = _runtimeHost.GetLastJsError();
                System.Diagnostics.Debug.WriteLine($"[NativeScript] Script exception: {scriptEx.Message}");
                CrashDiagnostics.WriteExceptionReport("RunMainScript", scriptEx, null);
                var report = CrashDiagnostics.BuildErrorReport(scriptEx, jsError);
                await CrashDiagnostics.ShowCrashDialogAsync("Script Execution Error", report);
            }

#if DEBUG
            Windows.UI.Xaml.Media.CompositionTarget.Rendering += OnRenderFrame;
#endif

            if (Window.Current.Content == null)
            {
                Window.Current.Content = new Windows.UI.Xaml.Controls.TextBlock
                {
                    Text = "NativeScript runtime initialized but no UI was rendered.\n" +
                           "Check the Output window for JS errors.",
                    Margin = new Windows.UI.Xaml.Thickness(20),
                    TextWrapping = Windows.UI.Xaml.TextWrapping.Wrap,
                    FontSize = 16,
                };
            }

            if (!e.PrelaunchActivated)
            {
                Window.Current.Activate();
            }
        }

        private void OnSuspending(object sender, SuspendingEventArgs e)
        {
            var deferral = e.SuspendingOperation.GetDeferral();
#if DEBUG
            Windows.UI.Xaml.Media.CompositionTarget.Rendering -= OnRenderFrame;
#endif
            ApplicationData.Current.LocalSettings.Values[LastLaunchArgsKey] = string.Empty;
            _runtimeHost.Dispose();
            deferral.Complete();
        }

        private void OnUnhandledException(object sender, Windows.UI.Xaml.UnhandledExceptionEventArgs e)
        {
            e.Handled = true;
            var jsError = _runtimeHost.GetLastJsError();
            CrashDiagnostics.WriteExceptionReport(
                "Xaml.UnhandledException",
                e.Exception,
                "JsError=" + (jsError ?? "<none>"));

            var report = CrashDiagnostics.BuildErrorReport(e.Exception, jsError);
            var _ = CrashDiagnostics.ShowCrashDialogAsync(
                e.Message ?? "Unhandled exception", report);
        }

#if DEBUG
        private void OnRenderFrame(object sender, object e) => _runtimeHost.PumpDevtools();
#endif
    }
}
