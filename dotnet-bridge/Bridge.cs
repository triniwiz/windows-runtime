using System;
using System.Collections;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Threading;
using System.Threading.Tasks;

namespace NativeScriptBridge;

/// <summary>
/// Reflection-based .NET dispatch bridge — UTF-8 ABI edition.
///
/// Invoke() receives a UTF-8 byte slice (no string allocation on the Rust side),
/// dispatches via reflection, and serialises the response directly to a UTF-8
/// byte array via JsonSerializer.SerializeToUtf8Bytes().  This removes the
/// UTF-16 encode/decode round-trip of the original char* ABI.
///
/// Request JSON schema
/// -------------------
///   Static call:    { "assembly": "System", "typeName": "System.Diagnostics.Stopwatch", "method": "StartNew", "args": [] }
///   Constructor:    { "assembly": "System", "typeName": "System.Text.StringBuilder",    "method": ".ctor",    "args": [128] }
///   Instance call:  { "handle": 3, "method": "Stop",          "args": [] }
///   Property get:   { "assembly": "...", "typeName": "...",   "method": "get_Now",      "args": [] }
///   Property set:   { "handle": 3, "method": "set_IsEnabled", "args": [true] }
///   Release:        { "handle": 3, "method": "__release",     "args": [] }
///
/// Response JSON schema
/// --------------------
///   Primitive / string:   { "result": 42.5 }
///   Managed object:       { "result": { "__handle": 7, "__type": "System.Diagnostics.Stopwatch" } }
///   Array / enumerable:   { "result": [1, 2, 3] }
///   Error:                { "error": "Method not found" }
/// </summary>
public static class Bridge
{
    private static readonly ConcurrentDictionary<int, object?> s_handles = new();
    private static int s_nextHandle;

    private static readonly ConcurrentDictionary<string, Type?> s_typeCache = new();
    private static readonly ConcurrentDictionary<string, MethodInfo?> s_methodCache = new();
    private static readonly ConcurrentDictionary<string, PropertyInfo?> s_propertyCache = new();

    // ── exported entry points ────────────────────────────────────────────────

    /// Called by the Rust runtime.  Request arrives as a raw UTF-8 byte slice;
    /// response is written as a UTF-8 byte buffer allocated with Marshal.AllocHGlobal.
    [UnmanagedCallersOnly(EntryPoint = "Invoke",
        CallConvs = [typeof(System.Runtime.CompilerServices.CallConvCdecl)])]
    public static unsafe int Invoke(
        byte* requestPtr, int requestLen,
        byte** responsePtr, int* responseLenPtr)
    {
        byte[] responseBytes;
        try
        {
            var requestSpan = new ReadOnlySpan<byte>(requestPtr, requestLen);
            var req = JsonSerializer.Deserialize<InvokeRequest>(requestSpan, JsonOptions.Default)!;
            var result = Dispatch(req);
            responseBytes = JsonSerializer.SerializeToUtf8Bytes(result, JsonOptions.Default);
        }
        catch (Exception ex)
        {
            var errResult = new InvokeResult(null, Unwrap(ex).Message);
            responseBytes = JsonSerializer.SerializeToUtf8Bytes(errResult, JsonOptions.Default);
        }

        WriteResponse(responseBytes, responsePtr, responseLenPtr);
        return 0;
    }

    /// Frees the response buffer allocated by Invoke.
    [UnmanagedCallersOnly(EntryPoint = "Free",
        CallConvs = [typeof(System.Runtime.CompilerServices.CallConvCdecl)])]
    public static unsafe void Free(byte* ptr)
    {
        if (ptr != null)
            Marshal.FreeHGlobal((IntPtr)ptr);
    }

    // ── dispatch ─────────────────────────────────────────────────────────────

    private static InvokeResult Dispatch(InvokeRequest req)
    {
        if (req.Method == "__release" && req.Handle.HasValue)
        {
            s_handles.TryRemove(req.Handle.Value, out _);
            return new InvokeResult(null, null);
        }

        // Return reflection metadata so JS can distinguish methods from properties.
        if (req.Method == "__members__")
        {
            var t = req.Handle.HasValue
                ? (s_handles.TryGetValue(req.Handle.Value, out var h) ? h?.GetType() : null)
                : ResolveType(req.Assembly, req.TypeName);
            if (t is null) return new InvokeResult(null, $"Type not found: {req.TypeName}");
            var inst   = BindingFlags.Public | BindingFlags.Instance;
            var stat   = BindingFlags.Public | BindingFlags.Static;
            var members = new
            {
                methods          = t.GetMethods(inst).Where(m => !m.IsSpecialName).Select(m => m.Name).Distinct().ToArray(),
                properties       = t.GetProperties(inst).Select(p => p.Name).Distinct().ToArray(),
                staticMethods    = t.GetMethods(stat).Where(m => !m.IsSpecialName).Select(m => m.Name).Distinct().ToArray(),
                staticProperties = t.GetProperties(stat).Select(p => p.Name).Distinct().ToArray(),
            };
            return new InvokeResult(JsonSerializer.SerializeToElement(members, JsonOptions.Default), null);
        }

        object? target = null;
        Type? type = null;

        if (req.Handle.HasValue)
        {
            if (!s_handles.TryGetValue(req.Handle.Value, out target))
                return new InvokeResult(null, $"Invalid handle {req.Handle.Value}");
            type = target?.GetType();
        }
        else
        {
            type = ResolveType(req.Assembly, req.TypeName);
            if (type is null)
                return new InvokeResult(null, $"Type not found: {req.TypeName}");
        }

        if (type is null)
            return new InvokeResult(null, "Cannot determine target type");

        var method = req.Method ?? throw new ArgumentException("Method is required");

        // ── Constructor ──────────────────────────────────────────────────────
        if (method == ".ctor")
        {
            var argElements = req.Args ?? [];
            var ctors = type.GetConstructors(BindingFlags.Public | BindingFlags.Instance);
            var ctor = ctors.FirstOrDefault(c => c.GetParameters().Length == argElements.Count)
                    ?? ctors.FirstOrDefault();
            if (ctor is null)
                return new InvokeResult(null, $"No public constructor found on {type.FullName}");
            var pars = ctor.GetParameters();
            var ctorArgs = new object?[pars.Length];
            for (int i = 0; i < pars.Length && i < argElements.Count; i++)
                ctorArgs[i] = Coerce(argElements[i], pars[i].ParameterType);
            return Box(ctor.Invoke(ctorArgs));
        }

        var isStatic = target is null;
        var flags = (isStatic ? BindingFlags.Static : BindingFlags.Instance)
                  | BindingFlags.Public;

        // ── Property getter / setter ─────────────────────────────────────────
        if (method.StartsWith("get_", StringComparison.Ordinal))
        {
            var prop = GetCachedProperty(type, method[4..], flags);
            if (prop is not null)
                return Box(prop.GetValue(target));
        }
        if (method.StartsWith("set_", StringComparison.Ordinal) && req.Args?.Count == 1)
        {
            var prop = GetCachedProperty(type, method[4..], flags);
            if (prop is not null)
            {
                prop.SetValue(target, Coerce(req.Args[0], prop.PropertyType));
                return new InvokeResult(null, null);
            }
        }

        // ── Regular method ───────────────────────────────────────────────────
        var argElems = req.Args ?? [];
        var mi = FindMethod(type, method, argElems.Count, flags);
        if (mi is null)
            return new InvokeResult(null,
                $"Method '{method}' ({argElems.Count} args) not found on {type.FullName}");

        var parameters = mi.GetParameters();
        var callArgs = new object?[parameters.Length];
        for (int i = 0; i < parameters.Length && i < argElems.Count; i++)
            callArgs[i] = Coerce(argElems[i], parameters[i].ParameterType);

        return Box(AwaitIfTask(mi.Invoke(target, callArgs)));
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    private static object? AwaitIfTask(object? value)
    {
        if (value is null) return null;
        var t = value.GetType();

        if (value is Task task)
        {
            task.Wait();
            if (task.IsFaulted)
                throw task.Exception!.InnerException ?? task.Exception;
            return t.GetProperty("Result")?.GetValue(value);
        }

        if (t.IsGenericType && t.GetGenericTypeDefinition() == typeof(ValueTask<>))
        {
            var innerTask = (Task)t.GetMethod("AsTask")!.Invoke(value, null)!;
            innerTask.Wait();
            if (innerTask.IsFaulted)
                throw innerTask.Exception!.InnerException ?? innerTask.Exception;
            return innerTask.GetType().GetProperty("Result")?.GetValue(innerTask);
        }

        if (value is System.Threading.Tasks.ValueTask vt)
        {
            vt.AsTask().Wait();
            return null;
        }

        return value;
    }

    private static Type? ResolveType(string? assemblyName, string? typeName)
    {
        if (string.IsNullOrEmpty(typeName)) return null;
        var key = string.IsNullOrEmpty(assemblyName) ? typeName : $"{assemblyName}|{typeName}";
        return s_typeCache.GetOrAdd(key, _ => ResolveTypeCore(assemblyName, typeName));
    }

    private static Type? ResolveTypeCore(string? assemblyName, string? typeName)
    {
        var fqn = string.IsNullOrEmpty(assemblyName) ? typeName! : $"{typeName}, {assemblyName}";
        var t = Type.GetType(fqn);
        if (t is not null) return t;

        foreach (var asm in AppDomain.CurrentDomain.GetAssemblies())
        {
            t = asm.GetType(typeName!);
            if (t is not null) return t;
        }

        if (!string.IsNullOrEmpty(assemblyName))
        {
            try
            {
                var asm = Assembly.Load(assemblyName);
                t = asm.GetType(typeName!);
                if (t is not null) return t;
            }
            catch { }
        }

        return null;
    }

    private static MethodInfo? FindMethod(Type type, string name, int argCount, BindingFlags flags)
    {
        var key = $"{type.FullName}|{name}|{argCount}|{(int)flags}";
        return s_methodCache.GetOrAdd(key, _ => FindMethodCore(type, name, argCount, flags));
    }

    private static MethodInfo? FindMethodCore(Type type, string name, int argCount, BindingFlags flags)
    {
        var match = Array.Find(
            type.GetMethods(flags),
            m => m.Name == name && !m.IsGenericMethod && m.GetParameters().Length == argCount);
        if (match is not null) return match;
        return type.GetMethod(name, flags);
    }

    private static PropertyInfo? GetCachedProperty(Type type, string name, BindingFlags flags)
    {
        var key = $"{type.FullName}|{name}|{(int)flags}";
        return s_propertyCache.GetOrAdd(key, _ => type.GetProperty(name, flags));
    }

    private static object? Coerce(JsonElement el, Type targetType)
    {
        if (el.ValueKind == JsonValueKind.Null) return null;
        if (el.ValueKind == JsonValueKind.Object && el.TryGetProperty("__handle", out var h))
        {
            s_handles.TryGetValue(h.GetInt32(), out var obj);
            return obj;
        }
        return JsonSerializer.Deserialize(el.GetRawText(), targetType, JsonOptions.Default);
    }

    private static InvokeResult Box(object? value)
    {
        if (value is null) return new InvokeResult(null, null);

        var t = value.GetType();

        if (t.IsPrimitive || t == typeof(string) || t == typeof(decimal)
            || t == typeof(DateTime) || t == typeof(DateTimeOffset)
            || t == typeof(TimeSpan) || t == typeof(Guid))
        {
            return new InvokeResult(
                JsonSerializer.SerializeToElement(value, t, JsonOptions.Default), null);
        }

        // Arrays and IEnumerable — serialise as JSON arrays.
        if (t.IsArray || (t != typeof(string) && value is IEnumerable enumerable))
        {
            try
            {
                var list = new List<object?>();
                foreach (var item in (IEnumerable)value) list.Add(item);
                return new InvokeResult(
                    JsonSerializer.SerializeToElement(list, typeof(List<object?>), JsonOptions.Default),
                    null);
            }
            catch { }
        }

        var id = Interlocked.Increment(ref s_nextHandle);
        s_handles[id] = value;
        return new InvokeResult(
            JsonSerializer.SerializeToElement(
                new { __handle = id, __type = t.FullName }, JsonOptions.Default),
            null);
    }

    private static Exception Unwrap(Exception ex) =>
        ex is TargetInvocationException { InnerException: { } inner } ? Unwrap(inner) : ex;

    private static unsafe void WriteResponse(byte[] json, byte** responsePtr, int* responseLenPtr)
    {
        var ptr = (byte*)Marshal.AllocHGlobal(json.Length + 1);
        Marshal.Copy(json, 0, (IntPtr)ptr, json.Length);
        ptr[json.Length] = 0;
        *responsePtr = ptr;
        *responseLenPtr = json.Length;
    }
}

// ── JSON types ────────────────────────────────────────────────────────────────

internal sealed record InvokeRequest(
    string? Assembly,
    string? TypeName,
    string? Method,
    int? Handle,
    List<JsonElement>? Args
);

internal sealed record InvokeResult(JsonElement? Result, string? Error);

internal static class JsonOptions
{
    internal static readonly JsonSerializerOptions Default = new()
    {
        PropertyNameCaseInsensitive = true,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
    };
}
