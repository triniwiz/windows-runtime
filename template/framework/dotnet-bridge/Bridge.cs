using System;
using System.Collections.Concurrent;
using System.Reflection;
using System.Runtime.InteropServices;
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
