using System;
using System.Runtime.InteropServices;
using System.IO;

class TestAppHost
{
    private const string NativeScriptLibrary = "nativescript";

    [DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_init))]
    static extern long runtime_init([MarshalAs(UnmanagedType.LPUTF8Str)] string entry);

    [DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_deinit))]
    static extern void runtime_deinit(long runtime);

    [DllImport(NativeScriptLibrary, EntryPoint = nameof(runtime_runscript))]
    static extern void runtime_runscript(long runtime, [MarshalAs(UnmanagedType.LPUTF8Str)] string script, [MarshalAs(UnmanagedType.LPUTF8Str)] string filename);

    static void Main(string[] args)
    {
        // If the generated XAML entry point exists (TestApp.Program.Main), invoke
        // it so the real UI thread is started. Fall back to the console-style
        // runtime host when that's not possible.
        try
        {
            var asm = System.Reflection.Assembly.GetEntryAssembly() ?? System.Reflection.Assembly.GetExecutingAssembly();
            var programType = asm?.GetType("TestApp.Program");
            if (programType == null)
            {
                // fallback: search all types for Program
                foreach (var t in asm.GetTypes())
                {
                    if (t.Name == "Program")
                    {
                        programType = t;
                        break;
                    }
                }
            }
            if (programType != null)
            {
                var mi = programType.GetMethod("Main", System.Reflection.BindingFlags.Static | System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Public);
                if (mi != null)
                {
                    mi.Invoke(null, new object[] { args });
                    return;
                }
            }
        }
        catch (Exception ex)
        {
            Console.WriteLine("Failed to invoke generated UI entrypoint, falling back: " + ex.Message);
        }

        // Fallback: initialize runtime directly (console-style host)
        var baseDir = AppContext.BaseDirectory;
        var lowerEntry = Path.Combine(baseDir, "app", "main.js");
        var upperEntry = Path.Combine(baseDir, "App", "main.js");
        string entry = File.Exists(lowerEntry) ? lowerEntry : upperEntry;

        long runtime = runtime_init(AppContext.BaseDirectory);
        try
        {
            var script = File.ReadAllText(Path.GetFullPath(entry));
            runtime_runscript(runtime, script, Path.GetFileName(entry));
        }
        catch (Exception ex)
        {
            Console.WriteLine("Runtime execution failed: " + ex);
        }
        finally
        {
            runtime_deinit(runtime);
        }
    }
}
