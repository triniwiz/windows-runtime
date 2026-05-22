using System;
using System.Buffers;
using System.Collections.Generic;
using System.Reflection;
using System.Runtime.InteropServices;

namespace NativeScriptBridge;

public static partial class Bridge
{
    [UnmanagedCallersOnly(EntryPoint = "InvokeBinary",
        CallConvs = [typeof(System.Runtime.CompilerServices.CallConvCdecl)])]
    public static unsafe int InvokeBinary(
        byte* requestPtr, int requestLen,
        byte** responsePtr, int* responseLenPtr)
    {
        try
        {
            var r   = new BinReader(new ReadOnlySpan<byte>(requestPtr, requestLen));
            var res = DispatchBin(ref r);
            var buf = new ArrayBufferWriter<byte>(128);
            res.WriteAsBin(buf);
            WriteUnmanaged(buf.WrittenSpan, responsePtr, responseLenPtr);
        }
        catch (Exception ex)
        {
            WriteBinError(Unwrap(ex).Message, responsePtr, responseLenPtr);
        }
        return 0;
    }

    internal static DispatchResult DispatchBin(ref BinReader r)
    {
        var op = r.ReadByte();

        if (op == 0x04) // release
        {
            s_handles.TryRemove(r.ReadI32(), out _);
            return DispatchResult.Void;
        }

        if (op == 0x05) // members by handle
        {
            var h = r.ReadI32();
            if (!s_handles.TryGetValue(h, out var obj))
                throw new KeyNotFoundException($"Invalid handle {h}");
            return BuildMembersResult(
                obj?.GetType() ?? throw new InvalidOperationException("Handle is null"));
        }

        if (op == 0x06) // members by type
        {
            var typeName = r.ReadString16();
            var assembly = r.ReadString16();
            var type = ResolveType(NullIfEmpty(assembly), typeName)
                ?? throw new TypeLoadException($"Type not found: {typeName} (assembly: {assembly})");
            return BuildMembersResult(type);
        }

        if (op == 0x01) // instance call
        {
            var handle = r.ReadI32();
            if (!s_handles.TryGetValue(handle, out var target))
                throw new KeyNotFoundException($"Invalid handle {handle}");
            var type   = target?.GetType() ?? throw new InvalidOperationException("Handle is null");
            var method = r.ReadString16();
            var args   = r.ReadArgs();

            if (method == "__dotnet_await__" && args.Length == 2
                && args[0] is int resolveId && args[1] is int rejectId)
            {
                ScheduleTaskContinuation(handle, resolveId, rejectId);
                return DispatchResult.Void;
            }

            return DispatchCallBin(target, type, method, args, isStatic: false);
        }

        if (op == 0x09) // create JS delegate
        {
            var delTypeName  = r.ReadString16(); // "" → System.Action
            var callbackId   = r.ReadI32();
            return CreateJsDelegate(delTypeName, callbackId);
        }

        // Static ops: 0x02 = call, 0x03 = constructor
        var typeNameS = r.ReadString16();
        var assemblyS = r.ReadString16();
        var typeS = ResolveType(NullIfEmpty(assemblyS), typeNameS)
            ?? throw new TypeLoadException($"Type not found: {typeNameS} (assembly: {assemblyS})");

        if (op == 0x03) // constructor
        {
            var args  = r.ReadArgs();
            var entry = GetCachedCtor(typeS, args.Length);
            if (entry.Ctor is null)
                throw new MissingMethodException(
                    $"No public ctor on {typeS.FullName} for {args.Length} args");
            // ConstructorInfo.Invoke requires exact arg count — build a precise array.
            return Box(entry.Ctor.Invoke(BuildArgsBinExact(args, entry.Parameters)));
        }

        // op == 0x02: static call
        var methodS = r.ReadString16();
        var argsS   = r.ReadArgs();
        return DispatchCallBin(null, typeS, methodS, argsS, isStatic: true);
    }

    private static DispatchResult DispatchCallBin(
        object? target, Type type, string method, object?[] args, bool isStatic)
    {
        var flags = (isStatic ? BindingFlags.Static : BindingFlags.Instance) | BindingFlags.Public;

        if (method.Length > 4
            && method[0] == 'g' && method[1] == 'e' && method[2] == 't' && method[3] == '_')
        {
            var prop = GetCachedProp(type, method, 4, flags);
            if (prop is not null) return Box(prop.GetValue(target));
        }

        if (method.Length > 4
            && method[0] == 's' && method[1] == 'e' && method[2] == 't' && method[3] == '_'
            && args.Length == 1)
        {
            var prop = GetCachedProp(type, method, 4, flags);
            if (prop is not null)
            {
                prop.SetValue(target, CoerceBin(args[0], prop.PropertyType));
                return DispatchResult.Void;
            }
        }

        var entry = GetCachedMethod(type, method, args.Length, flags);
        if (entry.Invoke is null)
            throw new MissingMethodException(
                $"Method '{method}' ({args.Length} args) not found on {type.FullName}");

        var builtArgs = BuildArgsBin(args, entry.Parameters);
        try   { return Box(AwaitIfTask(entry.Invoke(target, builtArgs))); }
        finally { if (builtArgs.Length > 0) ReturnArgs(builtArgs); }
    }

    private static object?[] BuildArgsBin(object?[] binArgs, ParameterInfo[] parameters)
    {
        if (parameters.Length == 0) return [];
        var result = ArrayPool<object?>.Shared.Rent(parameters.Length);
        for (int i = 0; i < parameters.Length && i < binArgs.Length; i++)
            result[i] = CoerceBin(binArgs[i], parameters[i].ParameterType);
        return result;
    }

    private static object?[] BuildArgsBinExact(object?[] binArgs, ParameterInfo[] parameters)
    {
        if (parameters.Length == 0) return [];
        var result = new object?[parameters.Length];
        for (int i = 0; i < parameters.Length && i < binArgs.Length; i++)
            result[i] = CoerceBin(binArgs[i], parameters[i].ParameterType);
        return result;
    }

    private static object? CoerceBin(object? value, Type targetType)
    {
        if (value is null) return null;
        if (value is HandleRef hr)
        {
            s_handles.TryGetValue(hr.Id, out var obj);
            return obj;
        }
        if (value is WinRtRef wr)
        {
            if (wr.Ptr == 0) return null;
            var nativePtr = new IntPtr((long)wr.Ptr);
            // 1. Typed QI first: works for COM/CsWinRT interface types that carry a
            //    [Guid] attribute.  More precise than a generic RCW for strongly-typed
            //    parameters such as Windows.UI.Xaml.UIElement.
            if (targetType != typeof(object) && targetType.GUID != Guid.Empty)
            {
                try { return Marshal.GetTypedObjectForIUnknown(nativePtr, targetType); }
                catch { }
            }
            // 2. Generic RCW: .NET WinRT interop calls IInspectable::GetRuntimeClassName
            //    and projects to the appropriate CsWinRT type automatically.
            //    Do NOT swallow the exception — a null return silently breaks the call
            //    downstream; a thrown exception surfaces a meaningful error instead.
            return Marshal.GetObjectForIUnknown(nativePtr);
        }
        if (value.GetType() == targetType) return value;
        try { return Convert.ChangeType(value, targetType); }
        catch { return value; }
    }

    private static string? NullIfEmpty(string s) => s.Length == 0 ? null : s;

    private static unsafe void WriteBinError(string msg, byte** outPtr, int* outLen)
    {
        var msgBytes = System.Text.Encoding.UTF8.GetByteCount(msg);
        var buf = new ArrayBufferWriter<byte>(5 + msgBytes);
        var w   = new BinWriter(buf);
        w.WriteByte(0xFF);
        w.WriteString32(msg);
        WriteUnmanaged(buf.WrittenSpan, outPtr, outLen);
    }
}
