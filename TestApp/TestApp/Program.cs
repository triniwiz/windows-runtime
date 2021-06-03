using System;
using System.IO;
using System.Runtime.InteropServices;
namespace TestApp
{
    class Program
       
    {

  
       
        [DllImport("nativescript.dll")]
        public static extern void hello();

        [DllImport("nativescript.dll")]
       public static extern Int64 runtime_init([MarshalAs(UnmanagedType.LPUTF8Str)] string entry);

        [DllImport("nativescript.dll")]
        public static extern void runtime_runscript(Int64 runtime, [MarshalAs(UnmanagedType.LPUTF8Str)] string entry);

        static void Main(string[] args)
        {   

            string entry = Path.Join(AppDomain.CurrentDomain.BaseDirectory, "App\\main.js");
            Int64 runtime =  runtime_init(AppContext.BaseDirectory);
            runtime_runscript(runtime, File.ReadAllText(Path.GetFullPath(entry)));
        }
    }
}
