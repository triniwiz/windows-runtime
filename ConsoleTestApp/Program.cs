using System.Runtime.InteropServices;

#if DEBUG
const string NativeScriptLibrary = "..\\libs\\devtools\\x64\\nativescript.dll";
#else
const string NativeScriptLibrary = "nativescript";
#endif

[DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_init))]
static extern Int64 runtime_init([MarshalAs(UnmanagedType.LPUTF8Str)] string entry);

[DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_deinit))]
static extern void runtime_deinit(Int64 runtime);

[DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_runscript))]
static extern void runtime_runscript(Int64 runtime, [MarshalAs(UnmanagedType.LPUTF8Str)] string entry, [MarshalAs(UnmanagedType.LPUTF8Str)] string filename);

#if DEBUG
[DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_devtools_start))]
static extern IntPtr runtime_devtools_start(long runtime, ushort port);

[DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_free_string))]
static extern void runtime_free_string(IntPtr ptr);
#endif

var baseDir = AppDomain.CurrentDomain.BaseDirectory;
var lowerEntry = Path.Combine(baseDir, "app", "main.js");
var upperEntry = Path.Combine(baseDir, "App", "main.js");
string entry = System.IO.File.Exists(lowerEntry) ? lowerEntry : upperEntry;
Int64 runtime = runtime_init(AppContext.BaseDirectory);
#if DEBUG
IntPtr devtoolsPtr = runtime_devtools_start(runtime, 42000);
if (devtoolsPtr != IntPtr.Zero)
{
	var ws = Marshal.PtrToStringUTF8(devtoolsPtr);
	runtime_free_string(devtoolsPtr);
	if (!string.IsNullOrEmpty(ws)) Console.WriteLine($"[NativeScript DevTools] {ws}");
}
#endif
var script = File.ReadAllText(Path.GetFullPath(entry));
runtime_runscript(runtime, script, Path.GetFileName(entry));

Console.WriteLine("Hello, World!");

runtime_deinit(runtime);
