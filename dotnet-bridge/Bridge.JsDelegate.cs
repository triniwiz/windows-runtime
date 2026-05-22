using System;
using System.Buffers;
using System.Linq;
using System.Linq.Expressions;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Threading;

namespace NativeScriptBridge;

public static partial class Bridge
{
    // opcode 0x09: given a delegate type name (or "" for System.Action) and a
    // JS callback id, compile a .NET delegate that serialises its parameters as
    // binary and calls back into V8 via the s_jsInvoker function pointer.

    private static DispatchResult CreateJsDelegate(string typeName, int callbackId)
    {
        var delegateType = string.IsNullOrEmpty(typeName)
            ? typeof(Action)
            : ResolveType(null, typeName)
              ?? throw new TypeLoadException($"Delegate type not found: {typeName}");

        var invokeMethod = delegateType.GetMethod("Invoke")
            ?? throw new MissingMethodException($"No Invoke method on {delegateType}");

        var parameters  = invokeMethod.GetParameters();
        var returnType  = invokeMethod.ReturnType;

        var paramExprs = parameters
            .Select((p, i) => Expression.Parameter(p.ParameterType, p.Name ?? $"p{i}"))
            .ToArray();

        // object?[] args = new object?[] { (object?)p0, (object?)p1, … }
        var objArgs   = paramExprs.Select(p => (Expression)Expression.Convert(p, typeof(object)));
        var argsArray = Expression.NewArrayInit(typeof(object), objArgs);

        var callMethod = typeof(Bridge).GetMethod(
            nameof(CallJsCallback),
            BindingFlags.Static | BindingFlags.NonPublic)!;

        var callExpr = Expression.Call(callMethod, Expression.Constant(callbackId), argsArray);

        Expression body = returnType == typeof(void)
            ? callExpr
            : (Expression)Expression.Block(returnType, callExpr, Expression.Default(returnType));

        var del = Expression.Lambda(delegateType, body, paramExprs).Compile();
        return Box(del);
    }

    internal static unsafe void CallJsCallback(int id, object?[] args)
    {
        if (s_jsInvoker == null) return;

        var buf = new ArrayBufferWriter<byte>(64);
        var w   = new BinWriter(buf);
        w.WriteByte((byte)Math.Min(args.Length, 255));
        foreach (var arg in args)
            WriteCallbackArg(ref w, arg);

        var bytes   = buf.WrittenSpan;
        byte* respPtr = null;
        int   respLen = 0;
        fixed (byte* p = bytes)
            s_jsInvoker(id, p, bytes.Length, &respPtr, &respLen);

        // Response is only set for non-void delegates; free it when present.
        if (respPtr != null && respLen > 0)
            Marshal.FreeHGlobal((IntPtr)respPtr);
    }

    // Serialises a single delegate argument in response-binary tag format so
    // the Rust side can reuse its existing bin_read_value parser.
    private static void WriteCallbackArg(ref BinWriter w, object? arg)
    {
        if (arg is null)    { w.WriteByte(0x00); return; }
        if (arg is bool b)  { w.WriteByte(b ? (byte)0x02 : (byte)0x01); return; }
        if (arg is int  i)  { w.WriteByte(0x03); w.WriteI32(i); return; }
        if (arg is uint u)  { w.WriteByte(0x03); w.WriteI32((int)u); return; }
        if (arg is long l)  { w.WriteByte(0x04); w.WriteF64((double)l); return; }
        if (arg is float f) { w.WriteByte(0x04); w.WriteF64((double)f); return; }
        if (arg is double d){ w.WriteByte(0x04); w.WriteF64(d); return; }
        if (arg is string s){ w.WriteByte(0x05); w.WriteString32(s); return; }

        // Object: box in the handle map and send as a handle reference.
        // The JS side receives {__handle, __type} which can be turned into a
        // proxy via NSWinRT.dotnet.fromHandle(...).
        var handleId = Interlocked.Increment(ref s_nextHandle);
        s_handles[handleId] = arg;
        var typeName = arg.GetType().FullName ?? arg.GetType().Name;
        w.WriteByte(0x06);
        w.WriteI32(handleId);
        w.WriteString16(typeName); // u16 length prefix (matches bin_read_value tag 0x06)
    }
}
