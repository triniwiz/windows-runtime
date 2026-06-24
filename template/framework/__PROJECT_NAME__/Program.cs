using System;
using System.Threading;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using WinRT;

// Windows-only WinUI desktop app. The custom entry point (DISABLE_XAML_GENERATED_MAIN)
// has no platform attribute, so CA1416 treats every Windows API call site as "reachable
// on all platforms." Declaring the assembly's supported platform once tells the analyzer
// the whole assembly only runs on Windows >= 17763, silencing those false positives.
[assembly: System.Runtime.Versioning.SupportedOSPlatform("windows10.0.17763.0")]

namespace __PROJECT_NAME__
{
    public static class Program
    {
        [System.STAThread]
        public static void Main(string[] args)
        {
            ComWrappersSupport.InitializeComWrappers();

            Application.Start((_callbackParams) =>
            {
                var dispatcherQueue = DispatcherQueue.GetForCurrentThread();
                if (dispatcherQueue != null)
                {
                    SynchronizationContext.SetSynchronizationContext(new DispatcherQueueSynchronizationContext(dispatcherQueue));
                }

				new App();
            });
        }
    }
}
