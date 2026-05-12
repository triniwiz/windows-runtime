using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using Windows.ApplicationModel;
using Windows.ApplicationModel.Activation;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Navigation;


namespace TestApp
{
    /// <summary>
    /// Provides application-specific behavior to supplement the default Application class.
    /// </summary>
    /// 

    sealed partial class App : Application
    {
        [DllImport("kernel32.dll")]
        private static extern bool AttachConsole(int dwProcessId);
        private const int ATTACH_PARENT_PROCESS = -1;
        private const string LastLaunchArgsKey = "LastLaunchArgs";
        private readonly RuntimeHost _runtimeHost = new RuntimeHost();
        private string _lastLaunchArgs = string.Empty;

        public App()
        {
            this.InitializeComponent();
            this.Suspending += OnSuspending;
        }

        /// <summary>
        /// Invoked when the application is launched normally by the end user.  Other entry points
        /// will be used such as when the application is launched to open a specific file.
        /// </summary>
        /// <param name="e">Details about the launch request and process.</param>
        protected override void OnLaunched(LaunchActivatedEventArgs e)
        {
            _lastLaunchArgs = e.Arguments ?? string.Empty;

            // Attach to parent console (if any) so early logs go to console when
            // the app is launched from a terminal. Runtime must start on the
            // main thread; perform synchronous startup and keep best-effort logging.
            AttachConsole(ATTACH_PARENT_PROCESS);
            LogSync("[TestApp] OnLaunched: starting runtime initialization");
            try
            {
                _runtimeHost.Initialize();
                LogSync("[TestApp] Runtime initialized");

                _runtimeHost.RunMainScript();
                LogSync("[TestApp] RunMainScript completed");
            }
            catch (Exception ex)
            {
                LogSync($"[TestApp] Runtime startup failed: {ex}");
            }

#if DEBUG
            Windows.UI.Xaml.Media.CompositionTarget.Rendering += OnRenderFrame;
#endif

            if (e.PreviousExecutionState == ApplicationExecutionState.Terminated
                && ApplicationData.Current.LocalSettings.Values.TryGetValue(LastLaunchArgsKey, out object value))
            {
                _lastLaunchArgs = value as string ?? _lastLaunchArgs;
            }

            if (!e.PrelaunchActivated)
            {
                Window.Current.Activate();
            }
        }

        private static void LogSync(string message)
        {
            try
            {
                var timestamped = $"{DateTime.UtcNow:O} {message}\r\n";

                // Best-effort: try app LocalFolder path, then temp folder, then Debug.
                try
                {
                    var localFolder = ApplicationData.Current.LocalFolder;
                    var localPath = localFolder.Path; // best-effort synchronous write
                    var localFile = System.IO.Path.Combine(localPath, "ns_testapp.log");
                    System.IO.File.AppendAllText(localFile, timestamped);
                }
                catch
                {
                    // Ignore failures to write to LocalFolder.
                }

                try
                {
                    var tmp = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "ns_testapp_host.log");
                    System.IO.File.AppendAllText(tmp, timestamped);
                }
                catch
                {
                }

                Debug.WriteLine(message);

                try { Console.WriteLine(timestamped); } catch { }
            }
            catch
            {
                // Swallow any logging exceptions.
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

#if DEBUG
        private void OnRenderFrame(object sender, object e) => _runtimeHost.PumpDevtools();
#endif
    }
}
