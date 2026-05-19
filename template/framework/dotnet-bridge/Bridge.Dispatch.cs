using System;
using System.Buffers;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Linq.Expressions;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace NativeScriptBridge;

public static partial class Bridge
{
    internal static DispatchResult Dispatch(InvokeRequest req)
    {
        var method = req.Method ?? throw new ArgumentException("Method is required");

        if (method == "__release" && req.Handle.HasValue)
        {
            s_handles.TryRemove(req.Handle.Value, out _);
            return DispatchResult.Void;
        }

        object? target = null;
        Type type;

        if (req.Handle.HasValue)
        {
            if (!s_handles.TryGetValue(req.Handle.Value, out target))
                throw new KeyNotFoundException($"Invalid handle {req.Handle.Value}");
            type = target?.GetType() ?? throw new InvalidOperationException("Handle refers to null");
        }
        else
        {
            type = ResolveType(req.Assembly, req.TypeName)
                ?? throw new TypeLoadException($"Type not found: {req.TypeName} (assembly: {req.Assembly})");
        }

        if (method == "__members__")
            return BuildMembersResult(type);

        if (method == ".ctor")
        {
            var argElems = req.Args ?? [];
            var entry    = GetCachedCtor(type, argElems.Length);
            if (entry.Ctor is null)
                throw new MissingMethodException(
                    $"No public constructor on {type.FullName} for {argElems.Length} args");
            // ConstructorInfo.Invoke validates args.Length == paramCount exactly — no pool.
            return Box(entry.Ctor.Invoke(BuildArgsExact(argElems, entry.Parameters)));
        }

        var isStatic = target is null;
        var flags    = (isStatic ? BindingFlags.Static : BindingFlags.Instance) | BindingFlags.Public;

        if (method.Length > 4)
        {
            if (method[0] == 'g' && method[1] == 'e' && method[2] == 't' && method[3] == '_')
            {
                var prop = GetCachedProp(type, method, 4, flags);
                if (prop is not null) return Box(prop.GetValue(target));
            }
            else if (method[0] == 's' && method[1] == 'e' && method[2] == 't' && method[3] == '_'
                     && req.Args?.Length == 1)
            {
                var prop = GetCachedProp(type, method, 4, flags);
                if (prop is not null)
                {
                    prop.SetValue(target, Coerce(req.Args[0], prop.PropertyType));
                    return DispatchResult.Void;
                }
            }
        }

        var args      = req.Args ?? [];
        var dispEntry = GetCachedMethod(type, method, args.Length, flags);
        if (dispEntry.Invoke is null)
            throw new MissingMethodException(
                $"Method '{method}' ({args.Length} args) not found on {type.FullName}");

        var builtArgs = BuildArgs(args, dispEntry.Parameters);
        try   { return Box(AwaitIfTask(dispEntry.Invoke(target, builtArgs))); }
        finally { if (builtArgs.Length > 0) ReturnArgs(builtArgs); }
    }

    private static DispatchEntry GetCachedMethod(Type type, string name, int argCount, BindingFlags flags)
        => s_methodCache.GetOrAdd(
            new MethodKey(type, name, argCount, flags),
            static k => BuildDispatchEntry(k.Type, k.Name, k.ArgCount, k.Flags));

    private static DispatchEntry BuildDispatchEntry(Type type, string name, int argCount, BindingFlags flags)
    {
        var mi = FindMethodCore(type, name, argCount, flags);
        if (mi is null) return DispatchEntry.Empty;

        var parameters = mi.GetParameters();

        try
        {
            var targetParam = Expression.Parameter(typeof(object), "t");
            var argsParam   = Expression.Parameter(typeof(object?[]), "a");

            var typedArgs = parameters.Select((p, i) =>
                (Expression)Expression.Convert(
                    Expression.ArrayIndex(argsParam, Expression.Constant(i)),
                    p.ParameterType)).ToArray();

            Expression body = mi.IsStatic
                ? Expression.Call(mi, typedArgs)
                : Expression.Call(Expression.Convert(targetParam, type), mi, typedArgs);

            if (body.Type == typeof(void))
                body = Expression.Block(typeof(object), body, Expression.Constant(null, typeof(object)));
            else if (body.Type != typeof(object))
                body = Expression.Convert(body, typeof(object));

            var fn = Expression.Lambda<Func<object?, object?[], object?>>(
                body, targetParam, argsParam).Compile();

            return new DispatchEntry(fn, parameters);
        }
        catch
        {
            // mi.Invoke validates args.Length == paramCount; slice the rented buffer if needed.
            int pc = parameters.Length;
            return new DispatchEntry(
                (t, a) => mi.Invoke(t, a.Length == pc ? a : a[..pc]),
                parameters);
        }
    }

    private static CtorEntry GetCachedCtor(Type type, int argCount)
        => s_ctorCache.GetOrAdd(new CtorKey(type, argCount), static k =>
        {
            var ctors = k.Type.GetConstructors(BindingFlags.Public | BindingFlags.Instance);
            var ctor  = Array.Find(ctors, c => c.GetParameters().Length == k.ArgCount)
                     ?? ctors.FirstOrDefault();
            return new CtorEntry(ctor, ctor?.GetParameters() ?? []);
        });

    private static PropertyInfo? GetCachedProp(Type type, string method, int prefixLen, BindingFlags flags)
        => s_propCache.GetOrAdd(
            new PropKey(type, method[prefixLen..], flags),
            static k => k.Type.GetProperty(k.Name, k.Flags));

    // Pooled: rented array passed to the compiled delegate (which accesses by index,
    // not by Length). Caller must return via ReturnArgs immediately after invoke.
    private static object?[] BuildArgs(JsonElement[] elements, ParameterInfo[] parameters)
    {
        if (parameters.Length == 0) return [];
        var args = ArrayPool<object?>.Shared.Rent(parameters.Length);
        for (int i = 0; i < parameters.Length && i < elements.Length; i++)
            args[i] = Coerce(elements[i], parameters[i].ParameterType);
        return args;
    }

    // Exact: required by ConstructorInfo.Invoke / MethodInfo.Invoke which validate
    // args.Length == paramCount and would throw on an oversized rented buffer.
    private static object?[] BuildArgsExact(JsonElement[] elements, ParameterInfo[] parameters)
    {
        if (parameters.Length == 0) return [];
        var args = new object?[parameters.Length];
        for (int i = 0; i < parameters.Length && i < elements.Length; i++)
            args[i] = Coerce(elements[i], parameters[i].ParameterType);
        return args;
    }

    private static void ReturnArgs(object?[] args) =>
        ArrayPool<object?>.Shared.Return(args, clearArray: true);

    private static object? Coerce(JsonElement el, Type targetType)
    {
        if (el.ValueKind == JsonValueKind.Null) return null;
        if (el.ValueKind == JsonValueKind.Object && el.TryGetProperty("__handle", out var h))
        {
            s_handles.TryGetValue(h.GetInt32(), out var obj);
            return obj;
        }
        return el.Deserialize(targetType, s_coerceOpts);
    }

    private static object? AwaitIfTask(object? value)
    {
        if (value is null) return null;

        if (value is Task task)
        {
            task.GetAwaiter().GetResult();
            return TaskResultCache.GetResult(task);
        }

        if (value is ValueTask vt)
        {
            vt.AsTask().GetAwaiter().GetResult();
            return null;
        }

        var type = value.GetType();
        if (type.IsGenericType && type.GetGenericTypeDefinition() == typeof(ValueTask<>))
        {
            var innerTask = ValueTaskAsTaskCache.AsTask(type, value);
            innerTask.GetAwaiter().GetResult();
            if (innerTask.IsFaulted) throw innerTask.Exception!.InnerException ?? innerTask.Exception;
            return TaskResultCache.GetResult(innerTask);
        }

        return value;
    }

    private static DispatchResult Box(object? value)
    {
        if (value is null) return DispatchResult.Void;
        var t = value.GetType();

        if (t.IsPrimitive || t == typeof(string)  || t == typeof(decimal)
            || t == typeof(DateTime) || t == typeof(DateTimeOffset)
            || t == typeof(TimeSpan) || t == typeof(Guid))
            return DispatchResult.Primitive(value, t);

        if (t.IsArray || (t != typeof(string) && value is IEnumerable))
        {
            try { return DispatchResult.Collection((IEnumerable)value); }
            catch { }
        }

        var id = Interlocked.Increment(ref s_nextHandle);
        s_handles[id] = value;
        return DispatchResult.Handle(id, t.FullName ?? t.Name);
    }

    internal static DispatchResult BuildMembersResult(Type t)
    {
        const BindingFlags inst = BindingFlags.Public | BindingFlags.Instance;
        const BindingFlags stat = BindingFlags.Public | BindingFlags.Static;

        var instProps = t.GetProperties(inst);
        var statProps = t.GetProperties(stat);

        return DispatchResult.Members(
            methods:               t.GetMethods(inst).Where(m => !m.IsSpecialName).Select(m => m.Name).Distinct().ToArray(),
            props:                 instProps.Where(p => p.GetGetMethod() != null).Select(p => p.Name).Distinct().ToArray(),
            staticMethods:         t.GetMethods(stat).Where(m => !m.IsSpecialName).Select(m => m.Name).Distinct().ToArray(),
            staticProps:           statProps.Where(p => p.GetGetMethod() != null).Select(p => p.Name).Distinct().ToArray(),
            readonlyProps:         instProps.Where(p => p.GetGetMethod() != null && p.GetSetMethod() == null).Select(p => p.Name).Distinct().ToArray(),
            readonlyStaticProps:   statProps.Where(p => p.GetGetMethod() != null && p.GetSetMethod() == null).Select(p => p.Name).Distinct().ToArray(),
            writeonlyProps:        instProps.Where(p => p.GetGetMethod() == null && p.GetSetMethod() != null).Select(p => p.Name).Distinct().ToArray(),
            writeonlyStaticProps:  statProps.Where(p => p.GetGetMethod() == null && p.GetSetMethod() != null).Select(p => p.Name).Distinct().ToArray()
        );
    }

    internal static Type? ResolveType(string? assemblyName, string? typeName)
    {
        if (string.IsNullOrEmpty(typeName)) return null;
        var key = string.IsNullOrEmpty(assemblyName) ? typeName : $"{assemblyName}|{typeName}";
        return s_typeCache.GetOrAdd(key, _ => ResolveTypeCore(assemblyName, typeName));
    }

    private static Type? ResolveTypeCore(string? assemblyName, string? typeName)
    {
        if (string.IsNullOrEmpty(typeName)) return null;

        // Fast/normal lookup first (assembly-qualified or plain type name).
        var fqn = string.IsNullOrEmpty(assemblyName) ? typeName! : $"{typeName}, {assemblyName}";
        var t = Type.GetType(fqn);
        if (t is not null) return t;

        t = Type.GetType(typeName);
        if (t is not null) return t;

        // Last segment (type name without namespace) used for loose matching below.
        var lastDot = typeName.LastIndexOf('.');
        var shortName = lastDot >= 0 ? typeName[(lastDot + 1)..] : typeName;

        // Search loaded assemblies quickly; prefer asm.GetType which is inexpensive.
        foreach (var asm in AppDomain.CurrentDomain.GetAssemblies())
        {
            try
            {
                t = asm.GetType(typeName);
                if (t is not null) return t;

                // Fall back to scanning exported types only when necessary. Some
                // assemblies throw on GetTypes() (ReflectionTypeLoadException) so
                // we catch and continue.
                foreach (var ty in asm.GetTypes())
                {
                    if (string.Equals(ty.FullName, typeName, StringComparison.Ordinal))
                        return ty;
                    // Short-name match only when input has no namespace prefix (e.g. "Stopwatch").
                    // Guarding on lastDot < 0 prevents matching unrelated types in other namespaces
                    // (e.g. "NativeScript.Widgets.FlexboxLayout" must not match a different
                    //  FlexboxLayout that happens to live in another namespace).
                    if (lastDot < 0 && string.Equals(ty.Name, shortName, StringComparison.Ordinal))
                        return ty;
                }
            }
            catch (ReflectionTypeLoadException) { }
            catch { }
        }

        // If an assembly name was provided try loading it explicitly and repeat
        // the above search inside that assembly only (less work than scanning all).
        if (!string.IsNullOrEmpty(assemblyName))
        {
            try
            {
                var asm = Assembly.Load(assemblyName);
                if (asm is not null)
                {
                    t = asm.GetType(typeName!);
                    if (t is not null) return t;
                    try
                    {
                        foreach (var ty in asm.GetTypes())
                        {
                            if (string.Equals(ty.FullName, typeName, StringComparison.Ordinal))
                                return ty;
                            if (lastDot < 0 && string.Equals(ty.Name, shortName, StringComparison.Ordinal))
                                return ty;
                        }
                    }
                    catch (ReflectionTypeLoadException) { }
                }
            }
            catch { }
        }

        return null;
    }

    private static MethodInfo? FindMethodCore(Type type, string name, int argCount, BindingFlags flags)
    {
        var match = Array.Find(type.GetMethods(flags),
            m => m.Name == name && !m.IsGenericMethod && m.GetParameters().Length == argCount);
        return match ?? type.GetMethod(name, flags);
    }

    internal static Exception Unwrap(Exception ex) =>
        ex is TargetInvocationException { InnerException: { } inner } ? Unwrap(inner) : ex;

    // Used by benchmarks to give a fair end-to-end comparison of protocols.

    internal static byte[] PipelineJson(ReadOnlySpan<byte> jsonRequest)
    {
        var req    = JsonSerializer.Deserialize(jsonRequest, BridgeJsonContext.Default.InvokeRequest)!;
        var result = Dispatch(req);
        var buf    = new ArrayBufferWriter<byte>(128);
        using var w = new Utf8JsonWriter(buf);
        w.WriteStartObject();
        w.WritePropertyName("result"u8);
        result.WriteTo(w, s_coerceOpts);
        w.WriteEndObject();
        w.Flush();
        return buf.WrittenSpan.ToArray();
    }

    internal static byte[] PipelineBinary(ReadOnlySpan<byte> binRequest)
    {
        var r      = new BinReader(binRequest);
        var result = DispatchBin(ref r);
        var buf    = new ArrayBufferWriter<byte>(128);
        result.WriteAsBin(buf);
        return buf.WrittenSpan.ToArray();
    }

    private static unsafe void WriteResult(DispatchResult res, byte** outPtr, int* outLen)
    {
        var buf = new ArrayBufferWriter<byte>(256);
        using var w = new Utf8JsonWriter(buf);
        w.WriteStartObject();
        w.WritePropertyName("result"u8);
        res.WriteTo(w, s_coerceOpts);
        w.WriteEndObject();
        w.Flush();
        WriteUnmanaged(buf.WrittenSpan, outPtr, outLen);
    }

    private static unsafe void WriteError(string msg, byte** outPtr, int* outLen)
    {
        var buf = new ArrayBufferWriter<byte>(128);
        using var w = new Utf8JsonWriter(buf);
        w.WriteStartObject();
        w.WriteString("error"u8, msg);
        w.WriteEndObject();
        w.Flush();
        WriteUnmanaged(buf.WrittenSpan, outPtr, outLen);
    }

    internal static unsafe void WriteUnmanaged(ReadOnlySpan<byte> bytes, byte** outPtr, int* outLen)
    {
        var p = (byte*)Marshal.AllocHGlobal(bytes.Length + 1);
        bytes.CopyTo(new Span<byte>(p, bytes.Length));
        p[bytes.Length] = 0;
        *outPtr  = p;
        *outLen  = bytes.Length;
    }
}
