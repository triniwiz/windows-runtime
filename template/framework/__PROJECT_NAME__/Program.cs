using System;
using System.Threading;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using WinRT;

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
