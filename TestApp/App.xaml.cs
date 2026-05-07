using System;
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

            _runtimeHost.Initialize();
            _runtimeHost.RunMainScript();

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
            ApplicationData.Current.LocalSettings.Values[LastLaunchArgsKey] = _lastLaunchArgs;
            _runtimeHost.Dispose();
            deferral.Complete();
        }
    }
}
