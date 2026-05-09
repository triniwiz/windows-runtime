using System;
using Windows.ApplicationModel;
using Windows.ApplicationModel.Activation;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Navigation;


namespace NativeScriptWindowsDemo
{
    /// <summary>
    /// Provides application-specific behavior to supplement the default Application class.
    /// </summary>
    /// 

    sealed partial class App : Application
    {
        private const string LastLaunchArgsKey = "LastLaunchArgs";
        private readonly RuntimeHost _runtimeHost = new RuntimeHost();
        private string _lastLaunchArgs = string.Empty;

        public App()
        {
            CrashDiagnostics.InstallGlobalHandlers();
            this.Suspending += OnSuspending;
            this.UnhandledException += OnUnhandledException;
        }

        /// <summary>
        /// Invoked when the application is launched normally by the end user.  Other entry points
        /// will be used such as when the application is launched to open a specific file.
        /// </summary>
        /// <param name="e">Details about the launch request and process.</param>
        protected override void OnLaunched(LaunchActivatedEventArgs e)
        {
            _lastLaunchArgs = e.Arguments ?? string.Empty;

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

            if (e.PreviousExecutionState == ApplicationExecutionState.Terminated
                && ApplicationData.Current.LocalSettings.Values.TryGetValue(LastLaunchArgsKey, out object value))
            {
                _lastLaunchArgs = value as string ?? _lastLaunchArgs;
            }

            // If JS did not set a root view, show a fallback so the window is not blank.
            // JS errors will appear in the VS Output window (stderr from the Rust runtime).
            if (Window.Current.Content == null)
            {
                Window.Current.Content = new Windows.UI.Xaml.Controls.TextBlock
                {
                    Text = "NativeScript runtime initialized but no UI was rendered.\n" +
                           "Check the VS Output window for JS errors.",
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

        /// <summary>
        /// Invoked when Navigation to a certain page fails
        /// </summary>
        /// <param name="sender">The Frame which failed navigation</param>
        /// <param name="e">Details about the navigation failure</param>
        void OnNavigationFailed(object sender, NavigationFailedEventArgs e)
        {
            throw new Exception("Failed to load Page " + e.SourcePageType.FullName);
        }

        /// <summary>
        /// Invoked when application execution is being suspended.  Application state is saved
        /// without knowing whether the application will be terminated or resumed with the contents
        /// of memory still intact.
        /// </summary>
        /// <param name="sender">The source of the suspend request.</param>
        /// <param name="e">Details about the suspend request.</param>
        private void OnSuspending(object sender, SuspendingEventArgs e)
        {
            var deferral = e.SuspendingOperation.GetDeferral();
#if DEBUG
            Windows.UI.Xaml.Media.CompositionTarget.Rendering -= OnRenderFrame;
#endif
            ApplicationData.Current.LocalSettings.Values[LastLaunchArgsKey] = _lastLaunchArgs;
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
