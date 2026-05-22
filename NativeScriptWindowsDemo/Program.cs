using Windows.UI.Xaml;

namespace NativeScriptWindowsDemo
{
    public static class Program
    {
        public static void Main(string[] args)
        {
            Application.Start(p =>
            {
                var app = new App();
            });
        }
    }
}
