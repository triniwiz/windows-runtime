using System;
using System.Buffers;
using System.Linq;
using System.Linq.Expressions;
using System.Reflection;
using System.Reflection.Emit;
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

        // Build a dynamic method that marshals parameters into an object[] and
        // calls back into JS via CallJsCallback/CallJsCallbackVoid. Expression
        // compilation occasionally generates incorrect code for some delegate
        // shapes; emitting IL here is reliable across signatures.
        var paramTypes = parameters.Select(p => p.ParameterType).ToArray();
        
        var dm = new DynamicMethod($"__ns_js_delegate_{callbackId}",
            returnType, paramTypes, typeof(Bridge).Module, skipVisibility: true);

        var il = dm.GetILGenerator();
        // local: object[] argsArray
        var argsLocal = il.DeclareLocal(typeof(object[]));
        il.Emit(OpCodes.Ldc_I4, paramTypes.Length);
        il.Emit(OpCodes.Newarr, typeof(object));
        il.Emit(OpCodes.Stloc, argsLocal);

        for (int i = 0; i < paramTypes.Length; i++)
        {
            il.Emit(OpCodes.Ldloc, argsLocal);
            il.Emit(OpCodes.Ldc_I4, i);
            // load argument (Ldarg_0..)
            switch (i + 1)
            {
                case 1: il.Emit(OpCodes.Ldarg_0); break;
                case 2: il.Emit(OpCodes.Ldarg_1); break;
                case 3: il.Emit(OpCodes.Ldarg_2); break;
                case 4: il.Emit(OpCodes.Ldarg_3); break;
                default: il.Emit(OpCodes.Ldarg, i + 1); break;
            }
            if (paramTypes[i].IsValueType) il.Emit(OpCodes.Box, paramTypes[i]);
            il.Emit(OpCodes.Stelem_Ref);
        }

        

        if (returnType == typeof(void))
        {
            var callVoid = typeof(Bridge).GetMethod(nameof(CallJsCallbackVoid), BindingFlags.Static | BindingFlags.NonPublic)!;
            il.Emit(OpCodes.Ldc_I4, callbackId);
            il.Emit(OpCodes.Ldloc, argsLocal);
            il.Emit(OpCodes.Call, callVoid);
            il.Emit(OpCodes.Ret);
        }
        else
        {
            var callObj = typeof(Bridge).GetMethod(nameof(CallJsCallback), BindingFlags.Static | BindingFlags.NonPublic)!;
            il.Emit(OpCodes.Ldc_I4, callbackId);
            il.Emit(OpCodes.Ldloc, argsLocal);
            il.Emit(OpCodes.Call, callObj);
            // ignore returned object and return default(returnType)
            if (returnType.IsValueType)
            {
                var loc = il.DeclareLocal(returnType);
                il.Emit(OpCodes.Ldloca_S, loc);
                il.Emit(OpCodes.Initobj, returnType);
                il.Emit(OpCodes.Ldloc, loc);
            }
            else
            {
                il.Emit(OpCodes.Ldnull);
            }
            il.Emit(OpCodes.Ret);
        }

        var del = dm.CreateDelegate(delegateType);
        return Box(del);
    }

    internal static unsafe object? CallJsCallback(int id, object?[] args)
    {
        if (s_jsInvoker == null) return null;

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

        object? result = null;
        // Response is only set for non-void delegates; parse and free when present.
        if (respPtr != null && respLen > 0)
        {
            try
            {
                var span = new ReadOnlySpan<byte>(respPtr, respLen);
                result = ParseCallbackResponse(span);
            }
            catch { result = null; }
            finally
            {
                Marshal.FreeHGlobal((IntPtr)respPtr);
            }
        }

        return result;
    }

    // Serialises a single delegate argument in response-binary tag format so
    // the Rust side can reuse its existing bin_read_value parser.
    private static void WriteCallbackArg(ref BinWriter w, object? arg)
    {
        // Allow existing handle references to be forwarded without re-boxing.
        if (arg is HandleRef hr)
        {
            w.WriteByte(0x06);
            w.WriteI32(hr.Id);
            try
            {
                if (s_handles.TryGetValue(hr.Id, out var obj) && obj != null)
                {
                    var objTypeName = obj.GetType().FullName ?? obj.GetType().Name;
                    w.WriteString16(objTypeName);
                }
                else
                {
                    w.WriteString16("");
                }
            }
            catch
            {
                w.WriteString16("");
            }
            if (Bridge.s_nativePtrs.TryGetValue(hr.Id, out var nativePtr))
            {
                w.WriteByte(1);
                w.WriteI64(nativePtr.ToInt64());
            }
            else
            {
                w.WriteByte(0);
            }
            return;
        }

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
        w.WriteString16(typeName);
        if (Bridge.s_nativePtrs.TryGetValue(handleId, out var nativePtr2)) {
            w.WriteByte(1);
            w.WriteI64(nativePtr2.ToInt64());
        } else {
            w.WriteByte(0);
        }
    }

    private static object? ParseCallbackResponse(ReadOnlySpan<byte> span)
    {
        if (span.IsEmpty) return null;
        var r = new BinReader(span);
        var tag = r.ReadByte();
        switch (tag)
        {
            case 0x00: return null;
            case 0x01: return (object)false;
            case 0x02: return (object)true;
            case 0x03: return (object)r.ReadI32();
            case 0x04: return (object)r.ReadF64();
            case 0x05: return (object)r.ReadString32();
            case 0x06: return (object)new HandleRef(r.ReadI32());
            case 0x0A: return (object)new WinRtRef(r.ReadI64());
            case 0x07: // array: u32 count + N tagged items
            {
                var count = (int)r.ReadU32();
                var arr = new object?[count];
                for (int i = 0; i < count; i++)
                {
                    var t = r.ReadByte();
                    switch (t)
                    {
                        case 0x00: arr[i] = null; break;
                        case 0x01: arr[i] = false; break;
                        case 0x02: arr[i] = true; break;
                        case 0x03: arr[i] = r.ReadI32(); break;
                        case 0x04: arr[i] = r.ReadF64(); break;
                        case 0x05: arr[i] = r.ReadString32(); break;
                        case 0x06: arr[i] = new HandleRef(r.ReadI32()); break;
                        case 0x0A: arr[i] = new WinRtRef(r.ReadI64()); break;
                        default: arr[i] = null; break;
                    }
                }
                return arr;
            }
            default: return null;
        }
    }

    internal static unsafe void CallJsCallbackVoid(int id, object?[] args)
    {
        CallJsCallback(id, args);
    }

    
}
