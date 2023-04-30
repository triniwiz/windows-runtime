using System.Runtime.InteropServices;


[DllImport("libs\\nativescript.dll")]
static extern Int64 runtime_init([MarshalAs(UnmanagedType.LPUTF8Str)] string entry);

[DllImport("libs\\nativescript.dll")]
static extern void runtime_deinit(Int64 runtime);

[DllImport("libs\\nativescript.dll")]
static extern void runtime_runscript(Int64 runtime, [MarshalAs(UnmanagedType.LPUTF8Str)] string entry);

string entry = Path.Join(AppDomain.CurrentDomain.BaseDirectory, "App\\main.js");
Int64 runtime = runtime_init(AppContext.BaseDirectory);
runtime_runscript(runtime, File.ReadAllText(Path.GetFullPath(entry)));

Console.WriteLine("Hello, World!");

runtime_deinit(runtime);
