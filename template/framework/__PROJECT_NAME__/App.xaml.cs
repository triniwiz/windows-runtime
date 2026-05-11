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

        protected override void OnLaunched(LaunchActivatedEventArgs e)
        {
            _runtimeHost.Initialize();
            try
            {
                _runtimeHost.RunMainScript();
            }
            catch (Exception scriptEx)
            {
                System.Diagnostics.Debug.WriteLine($"[NativeScript] Script exception: {scriptEx.Message}");
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
            CrashDiagnostics.WriteExceptionReport(
                "Xaml.UnhandledException",
                e.Exception,
                "Message=" + e.Message + "; Handled=" + e.Handled);
        }

#if DEBUG
        private void OnRenderFrame(object sender, object e) => _runtimeHost.PumpDevtools();
#endif
    }
}
