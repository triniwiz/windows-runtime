using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Runtime.Loader;
using System.Text.Json;

[assembly: System.Runtime.CompilerServices.InternalsVisibleTo("DotNetBridgeTests")]
[assembly: System.Runtime.CompilerServices.InternalsVisibleTo("DotNetBridgeBenchmarks")]

namespace NativeScriptBridge;

public static partial class Bridge
{
    internal static readonly ConcurrentDictionary<int, object?> s_handles = new();
    internal static int s_nextHandle;

    // Function pointer registered by the Rust runtime so managed delegates can
    // call back into V8 without a JSON round-trip.
    internal static unsafe delegate* unmanaged[Cdecl]<int, byte*, int, byte**, int*, void>
        s_jsInvoker;

    private static readonly ConcurrentDictionary<string, Type?> s_typeCache
        = new(StringComparer.Ordinal);
    private static readonly ConcurrentDictionary<MethodKey, DispatchEntry> s_methodCache = new();
    private static readonly ConcurrentDictionary<PropKey, PropertyInfo?> s_propCache = new();
    private static readonly ConcurrentDictionary<CtorKey, CtorEntry> s_ctorCache = new();
    // Cache of attempted assembly loads (simple name -> Assembly or null if not found).
    private static readonly ConcurrentDictionary<string, Assembly?> s_assemblyLoadCache
        = new(StringComparer.OrdinalIgnoreCase);
    // Directories to search for assemblies (initialized once in static ctor).
    private static readonly string[] s_assemblySearchDirs;

    private static readonly JsonSerializerOptions s_coerceOpts = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
    };

    internal static void ClearCaches()
    {
        s_typeCache.Clear();
        s_methodCache.Clear();
        s_propCache.Clear();
        s_ctorCache.Clear();
        s_handles.Clear();
        s_nextHandle = 0;
    }

    // Returns a JSON object mapping top-level namespace roots (e.g. "NativeScript")
    // to the assembly simple-name that most likely contains that namespace's types.
    // Reuses s_assemblySearchDirs so plugin and NuGet assemblies are included.
    public static string GetNamespaceAssemblyMapJson()
    {
        var map = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        try
        {
            foreach (var dir in s_assemblySearchDirs.Where(Directory.Exists))
            {
                try
                {
                    foreach (var file in Directory.EnumerateFiles(dir, "*.dll", SearchOption.TopDirectoryOnly))
                    {
                        try
                        {
                            var simple = Path.GetFileNameWithoutExtension(file);
                            if (string.IsNullOrEmpty(simple)) continue;

                            Assembly asm = AppDomain.CurrentDomain.GetAssemblies()
                                .FirstOrDefault(a => string.Equals(a.GetName().Name, simple, StringComparison.OrdinalIgnoreCase));
                            if (asm is null)
                            {
                                try { asm = AssemblyLoadContext.Default.LoadFromAssemblyPath(file); }
                                catch { continue; }
                            }

                            Type[] types;
                            try { types = asm.GetExportedTypes(); }
                            catch { types = Array.Empty<Type>(); }

                            foreach (var t in types)
                            {
                                if (string.IsNullOrEmpty(t.Namespace)) continue;
                                var root = t.Namespace.Split('.')[0];
                                if (!map.ContainsKey(root)) map[root] = asm.GetName().Name;
                            }
                        }
                        catch { }
                    }
                }
                catch { }
            }
        }
        catch { }
        try { return JsonSerializer.Serialize(map); } catch { return "{}"; }
    }

    static Bridge()
    {
        try
        {
            var baseDir = AppContext.BaseDirectory ?? AppDomain.CurrentDomain.BaseDirectory;
            var dirs = new List<string> { baseDir };

            // libs/ subtree relative to the bridge's own directory.
            var libs = Path.Combine(baseDir, "libs");
            if (Directory.Exists(libs))
            {
                dirs.Add(libs);
                try { dirs.AddRange(Directory.GetDirectories(libs, "*", SearchOption.AllDirectories)); } catch { }
            }

            // The bridge DLL lives at dotnet-bridge/publish/ inside the app output root.
            // Plugin assemblies (added via plugin.props) land at plugins/**/*.dll and NuGet
            // assemblies land at the app output root itself — both outside the bridge subtree.
            // Use the host process directory to reach them.
            try
            {
                var processDir = Path.GetDirectoryName(
                    System.Diagnostics.Process.GetCurrentProcess().MainModule?.FileName);
                if (!string.IsNullOrEmpty(processDir))
                {
                    dirs.Add(processDir);

                    // plugins/ subtree: CLI-managed plugin DLLs live here.
                    var pluginsDir = Path.Combine(processDir, "plugins");
                    if (Directory.Exists(pluginsDir))
                        try { dirs.AddRange(Directory.GetDirectories(pluginsDir, "*", SearchOption.AllDirectories)); } catch { }

                    // libs/ relative to the app root (alternative convention).
                    var processLibs = Path.Combine(processDir, "libs");
                    if (Directory.Exists(processLibs))
                        try { dirs.AddRange(Directory.GetDirectories(processLibs, "*", SearchOption.AllDirectories)); } catch { }
                }
            }
            catch { }

            s_assemblySearchDirs = dirs.Where(d => !string.IsNullOrEmpty(d)).Distinct(StringComparer.OrdinalIgnoreCase).ToArray();
            AppDomain.CurrentDomain.AssemblyResolve += OnAssemblyResolve;
        }
        catch
        {
            s_assemblySearchDirs = Array.Empty<string>();
        }
    }

    private static Assembly? OnAssemblyResolve(object? sender, ResolveEventArgs args)
    {
        try
        {
            var requested = new AssemblyName(args.Name).Name;
            if (string.IsNullOrEmpty(requested)) return null;

            if (s_assemblyLoadCache.TryGetValue(requested, out var cached)) return cached;

            // Check already-loaded assemblies first (avoid recursive Assembly.Load).
            var loadedAsm = AppDomain.CurrentDomain.GetAssemblies()
                .FirstOrDefault(a => string.Equals(a.GetName().Name, requested, StringComparison.OrdinalIgnoreCase));
            if (loadedAsm is not null)
            {
                s_assemblyLoadCache[requested] = loadedAsm;
                return loadedAsm;
            }

            // Try to locate a matching dll in known search directories.
            foreach (var dir in s_assemblySearchDirs)
            {
                try
                {
                    var candidate = Path.Combine(dir, requested + ".dll");
                    if (File.Exists(candidate))
                    {
                        try
                        {
                            var asm = AssemblyLoadContext.Default.LoadFromAssemblyPath(candidate);
                            s_assemblyLoadCache[requested] = asm;
                            return asm;
                        }
                        catch { }
                    }
                }
                catch { }
            }

            s_assemblyLoadCache[requested] = null;
        }
        catch { }
        return null;
    }

    [UnmanagedCallersOnly(EntryPoint = "RegisterJsCallback",
        CallConvs = [typeof(System.Runtime.CompilerServices.CallConvCdecl)])]
    public static unsafe int RegisterJsCallback(
        delegate* unmanaged[Cdecl]<int, byte*, int, byte**, int*, void> callback)
    {
        s_jsInvoker = callback;
        return 0;
    }

    [UnmanagedCallersOnly(EntryPoint = "Invoke",
        CallConvs = [typeof(System.Runtime.CompilerServices.CallConvCdecl)])]
    public static unsafe int Invoke(
        byte* requestPtr, int requestLen,
        byte** responsePtr, int* responseLenPtr)
    {
        try
        {
            var span = new ReadOnlySpan<byte>(requestPtr, requestLen);
            var req  = JsonSerializer.Deserialize(span, BridgeJsonContext.Default.InvokeRequest)!;
            WriteResult(Dispatch(req), responsePtr, responseLenPtr);
        }
        catch (Exception ex)
        {
            WriteError(Unwrap(ex).Message, responsePtr, responseLenPtr);
        }
        return 0;
    }

    [UnmanagedCallersOnly(EntryPoint = "Free",
        CallConvs = [typeof(System.Runtime.CompilerServices.CallConvCdecl)])]
    public static unsafe void Free(byte* ptr)
    {
        if (ptr != null) Marshal.FreeHGlobal((IntPtr)ptr);
    }
}
