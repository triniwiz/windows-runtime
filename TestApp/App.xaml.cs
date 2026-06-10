using System;
using System.Text.Json;
using Windows.ApplicationModel;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.Storage;

namespace TestApp
{
    sealed partial class App : Application
    {
        private const string LastLaunchArgsKey = "LastLaunchArgs";
        private readonly RuntimeHost _runtimeHost = new RuntimeHost();

        public static Window CurrentWindow { get; private set; }
        public Window MainWindow => CurrentWindow;
        public Window Window => CurrentWindow;

        enum AppEventKind
        {
            Activated = 1,
            Deactivated = 2,
            Shown = 3,
            Hidden = 4,
            UncaughtError = 5,
            Exit = 6
        }

        public App()
        {
			this.InitializeComponent();
			CurrentWindow ??= new Window();

            CrashDiagnostics.InstallGlobalHandlers();
            CurrentWindow.Closed += OnWindowClosed;
            CurrentWindow.Activated += OnWindowActivated;
            CurrentWindow.VisibilityChanged += OnWindowVisibilityChanged;
            this.UnhandledException += OnUnhandledException;
        }

        protected override async void OnLaunched(LaunchActivatedEventArgs e)
        {
            _runtimeHost.Initialize();

            // Capture before any await — continuations may resume on a thread pool thread.
            var dispatcherQueue = DispatcherQueue.GetForCurrentThread();

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
                    CrashDiagnostics.WriteToTraceLog(report);
                    await CrashDiagnostics.ShowCrashDialogAsync("JavaScript Error", report);
                }
            }
            catch (Exception scriptEx)
            {
                jsError = _runtimeHost.GetLastJsError();
                System.Diagnostics.Debug.WriteLine($"[NativeScript] Script exception: {scriptEx.Message}");
                CrashDiagnostics.WriteExceptionReport("RunMainScript", scriptEx, null);
                var report = CrashDiagnostics.BuildErrorReport(scriptEx, jsError);
                CrashDiagnostics.WriteToTraceLog(report);
                await CrashDiagnostics.ShowCrashDialogAsync("Script Execution Error", report);
            }

            void ShowMainWindow()
            {
                StartPump();

                // NativeScript (JS) creates, populates and activates the application Window.
                // Only activate this host-owned window if JS put content on it; otherwise leave it
                // hidden so we don't pop an empty second window.
                try
                {
                    if (CurrentWindow?.Content != null)
                    {
                        CurrentWindow.Activate();
                    }
                }
                catch { /* JS owns its own Window */ }
            }

            // After any await, the continuation may run on a thread pool thread.
            // Schedule all UI-thread-required operations through the dispatcher queue.
            if (dispatcherQueue?.HasThreadAccess == true)
            {
                ShowMainWindow();
            }
            else if (!(dispatcherQueue?.TryEnqueue(ShowMainWindow) ?? false))
            {
                ShowMainWindow();
            }
        }

        private void OnWindowClosed(object sender, WindowEventArgs e)
        {
            StopPump();
            // Fire the JS `exit` event while the V8 isolate is still alive (before Dispose).
            _runtimeHost.NotifyAppEvent((int)AppEventKind.Exit, null);
            ApplicationData.Current.LocalSettings.Values[LastLaunchArgsKey] = string.Empty;
            _runtimeHost.Dispose();
        }

        // WindowActivationState.Deactivated → lost focus (background); otherwise foreground/resume.
        private void OnWindowActivated(object sender, WindowActivatedEventArgs e)
        {
            var kind = e.WindowActivationState == WindowActivationState.Deactivated ? AppEventKind.Deactivated : AppEventKind.Activated;
            _runtimeHost.NotifyAppEvent((int)kind, null);
        }

        private void OnWindowVisibilityChanged(object sender, WindowVisibilityChangedEventArgs e)
        {
            var kind = e.Visible ? AppEventKind.Shown : AppEventKind.Hidden;
            _runtimeHost.NotifyAppEvent((int)kind, null);
        }

        private void OnUnhandledException(object sender, Microsoft.UI.Xaml.UnhandledExceptionEventArgs e)
        {
            e.Handled = true;
            var jsError = _runtimeHost.GetLastJsError();

            // Surface to the JS `uncaughtError` application event before reporting.
            var message = e.Message ?? jsError ?? "Unhandled exception";
            _runtimeHost.NotifyAppEvent((int)AppEventKind.UncaughtError, message);

            CrashDiagnostics.WriteExceptionReport(
                "Xaml.UnhandledException",
                e.Exception,
                "JsError=" + (jsError ?? "<none>"));

            var report = CrashDiagnostics.BuildErrorReport(e.Exception, jsError);
            CrashDiagnostics.WriteToTraceLog(report);
            var _ = CrashDiagnostics.ShowCrashDialogAsync(
                e.Message ?? "Unhandled exception", report);
        }

        // Pump cadence from two sources, both via the guarded SchedulePump(): CompositionTarget.Rendering
        // (display refresh rate; smooth JS-driven animations) + a low-frequency DispatcherQueueTimer
        // heartbeat (keeps timers/promises running when Rendering stops, e.g. minimised/occluded). The pump
        // runs OUTSIDE the render walk (TryEnqueue) so the JS it pumps can mutate the XAML tree without
        // tripping XAML's re-entrancy guard (which RoFailFasts with E_UNEXPECTED / 0xC000027B). _pumpQueued
        // keeps at most one pump in flight.
        private DispatcherQueue _dispatcherQueue;
        private DispatcherQueueTimer _pumpHeartbeat;
        private bool _pumpQueued;

        private void StartPump()
        {
            _dispatcherQueue = DispatcherQueue.GetForCurrentThread();
            CompositionTarget.Rendering -= OnRenderFrame;
            CompositionTarget.Rendering += OnRenderFrame;
            if (_pumpHeartbeat == null && _dispatcherQueue != null)
            {
                _pumpHeartbeat = _dispatcherQueue.CreateTimer();
                _pumpHeartbeat.Interval = TimeSpan.FromMilliseconds(100);
                _pumpHeartbeat.IsRepeating = true;
                _pumpHeartbeat.Tick += OnPumpHeartbeat;
                _pumpHeartbeat.Start();
            }
        }

        private void StopPump()
        {
            CompositionTarget.Rendering -= OnRenderFrame;
            if (_pumpHeartbeat != null)
            {
                _pumpHeartbeat.Stop();
                _pumpHeartbeat.Tick -= OnPumpHeartbeat;
                _pumpHeartbeat = null;
            }
        }

        // Inside XAML's render walk — only schedule; never pump or touch the tree here.
        private void OnRenderFrame(object sender, object e) => SchedulePump();

        private void OnPumpHeartbeat(DispatcherQueueTimer sender, object args) => SchedulePump();

        private void SchedulePump()
        {
            if (_pumpQueued)
            {
                return;
            }
            var dispatcherQueue = _dispatcherQueue;
            if (dispatcherQueue == null)
            {
                return;
            }
            _pumpQueued = true;
            dispatcherQueue.TryEnqueue(() =>
            {
                _pumpQueued = false;
                _runtimeHost.PumpTimers();
#if DEBUG
                _runtimeHost.PumpDevtools();
#endif
            });
        }
    }
}
