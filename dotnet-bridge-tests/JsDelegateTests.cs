using System;
using System.Buffers;
using System.Buffers.Binary;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using NativeScriptBridge;
using Xunit;

namespace DotNetBridgeTests;

// Tests for the .NET-bridge JS delegate path (Bridge.JsDelegate.cs):
//   opcode 0x09  — CreateJsDelegate
//   CallJsCallback — serialises args in response-binary tag format
//   WriteCallbackArg — per-type encoding
//
// All tests run on the same thread as the test host, which is the "JS thread"
// in the context of the real runtime.  s_jsInvoker is set to a static
// [UnmanagedCallersOnly] capture method so delegate invocations can be
// verified without a live V8 isolate.

[Collection("Bridge")]
public sealed class JsDelegateTests : IDisposable
{
    private static int    s_capturedId;
    private static int    s_captureCount;
    private static byte[] s_capturedArgs = [];

    [UnmanagedCallersOnly(CallConvs = [typeof(System.Runtime.CompilerServices.CallConvCdecl)])]
    private static unsafe void CaptureInvoker(
        int id, byte* argsPtr, int argsLen, byte** respPtr, int* respLen)
    {
        s_capturedId   = id;
        s_captureCount++;
        s_capturedArgs = new ReadOnlySpan<byte>(argsPtr, argsLen).ToArray();
        *respPtr = null;
        *respLen = 0;
    }

    public unsafe JsDelegateTests()
    {
        Bridge.ClearCaches();
        s_capturedId   = -1;
        s_captureCount = 0;
        s_capturedArgs = [];
        Bridge.s_jsInvoker = &CaptureInvoker;
    }

    public unsafe void Dispose()
    {
        Bridge.s_jsInvoker = null;
        Bridge.ClearCaches();
    }

    [Fact]
    public void CreateJsDelegate_SystemAction_ReturnsHandle()
    {
        var result = DispatchCreateDelegate("", callbackId: 1);
        Assert.Equal(DispatchKind.Handle, result.Kind());
        Release(result);
    }

    [Fact]
    public void CreateJsDelegate_EventHandler_ReturnsHandle()
    {
        // System.EventHandler is always available — non-generic, void (object, EventArgs)
        var result = DispatchCreateDelegate("System.EventHandler", callbackId: 2);
        Assert.Equal(DispatchKind.Handle, result.Kind());
        Release(result);
    }

    [Fact]
    public void CreateJsDelegate_UnknownType_Throws()
    {
        Assert.ThrowsAny<Exception>(() =>
            DispatchCreateDelegate("No.Such.Delegate", callbackId: 99));
    }

    [Fact]
    public void CreateJsDelegate_StoredObjectIsDelegate()
    {
        var result = DispatchCreateDelegate("", callbackId: 5);
        var handle = result.HandleId();
        Bridge.s_handles.TryGetValue(handle, out var obj);
        Assert.NotNull(obj);
        Assert.IsAssignableFrom<Delegate>(obj);
        Release(handle);
    }

    [Fact]
    public void CreateJsDelegate_MultipleCallsGetDistinctHandles()
    {
        var r1 = DispatchCreateDelegate("", callbackId: 10);
        var r2 = DispatchCreateDelegate("", callbackId: 11);
        Assert.NotEqual(r1.HandleId(), r2.HandleId());
        Release(r1);
        Release(r2);
    }

    [Fact]
    public void InvokeDelegate_SystemAction_FiresCallback()
    {
        const int id = 20;
        var action = CreateDelegate<Action>("", id);

        action();

        Assert.Equal(id, s_capturedId);
        Assert.Equal(1, s_captureCount);
        // arg buffer: [count=0]
        Assert.Equal(0, s_capturedArgs[0]);
    }

    [Fact]
    public void InvokeDelegate_NullArg_SerializesNullTag()
    {
        // Use Action<object?> — but ResolveType needs the right name.
        // Drive through CallJsCallback directly since it's callable from safe code.
        const int id = 30;
        Bridge.CallJsCallback(id, [null]);

        Assert.Equal(id, s_capturedId);
        Assert.Equal(1, s_capturedArgs[0]); // count
        Assert.Equal(0x00, s_capturedArgs[1]); // null tag
    }

    [Fact]
    public void InvokeDelegate_BoolFalse_SerializesFalseTag()
    {
        Bridge.CallJsCallback(31, [false]);
        Assert.Equal(1, s_capturedArgs[0]);
        Assert.Equal(0x01, s_capturedArgs[1]); // false tag
    }

    [Fact]
    public void InvokeDelegate_BoolTrue_SerializesTrueTag()
    {
        Bridge.CallJsCallback(32, [true]);
        Assert.Equal(1, s_capturedArgs[0]);
        Assert.Equal(0x02, s_capturedArgs[1]); // true tag
    }

    [Fact]
    public void InvokeDelegate_Int_SerializesI32Tag()
    {
        Bridge.CallJsCallback(33, [-42]);

        Assert.Equal(1, s_capturedArgs[0]);  // count
        Assert.Equal(0x03, s_capturedArgs[1]); // i32 tag
        Assert.Equal(-42, BinaryPrimitives.ReadInt32LittleEndian(s_capturedArgs.AsSpan(2)));
    }

    [Fact]
    public void InvokeDelegate_UInt_SerializesI32Tag()
    {
        Bridge.CallJsCallback(34, [(object)(uint)7u]);
        Assert.Equal(0x03, s_capturedArgs[1]);
        Assert.Equal(7, BinaryPrimitives.ReadInt32LittleEndian(s_capturedArgs.AsSpan(2)));
    }

    [Fact]
    public void InvokeDelegate_Double_SerializesF64Tag()
    {
        Bridge.CallJsCallback(35, [3.14]);

        Assert.Equal(0x04, s_capturedArgs[1]); // f64 tag
        var bits = BinaryPrimitives.ReadUInt64LittleEndian(s_capturedArgs.AsSpan(2));
        Assert.Equal(3.14, BitConverter.UInt64BitsToDouble(bits), precision: 10);
    }

    [Fact]
    public void InvokeDelegate_Float_SerializesF64Tag()
    {
        Bridge.CallJsCallback(36, [(object)1.5f]);
        Assert.Equal(0x04, s_capturedArgs[1]);
    }

    [Fact]
    public void InvokeDelegate_Long_SerializesF64Tag()
    {
        Bridge.CallJsCallback(37, [(object)100L]);
        Assert.Equal(0x04, s_capturedArgs[1]);
    }

    [Fact]
    public void InvokeDelegate_String_SerializesStringTag()
    {
        const string text = "hello";
        Bridge.CallJsCallback(38, [text]);

        Assert.Equal(1, s_capturedArgs[0]);
        Assert.Equal(0x05, s_capturedArgs[1]); // string tag
        var len = BinaryPrimitives.ReadUInt32LittleEndian(s_capturedArgs.AsSpan(2));
        Assert.Equal((uint)Encoding.UTF8.GetByteCount(text), len);
        var decoded = Encoding.UTF8.GetString(s_capturedArgs.AsSpan(6, (int)len));
        Assert.Equal(text, decoded);
    }

    [Fact]
    public void InvokeDelegate_String_Utf8Encoded()
    {
        const string text = "café";
        Bridge.CallJsCallback(39, [text]);
        Assert.Equal(0x05, s_capturedArgs[1]);
        var len = (int)BinaryPrimitives.ReadUInt32LittleEndian(s_capturedArgs.AsSpan(2));
        var decoded = Encoding.UTF8.GetString(s_capturedArgs.AsSpan(6, len));
        Assert.Equal(text, decoded);
    }

    [Fact]
    public void InvokeDelegate_Object_SerializesAsHandle()
    {
        var obj = new object();
        Bridge.CallJsCallback(40, [obj]);

        Assert.Equal(1, s_capturedArgs[0]);
        Assert.Equal(0x06, s_capturedArgs[1]); // handle tag

        // Handle id stored in s_handles
        var handleId = BinaryPrimitives.ReadInt32LittleEndian(s_capturedArgs.AsSpan(2));
        Assert.True(Bridge.s_handles.TryGetValue(handleId, out var stored));
        Assert.Same(obj, stored);
    }

    [Fact]
    public void InvokeDelegate_ObjectTypeName_EncodedAfterHandleId()
    {
        var obj = new System.Text.StringBuilder();
        Bridge.CallJsCallback(41, [obj]);

        Assert.Equal(0x06, s_capturedArgs[1]);
        // Skip handle id (4 bytes); next is u16 type name length
        var typeNameLen = BinaryPrimitives.ReadUInt16LittleEndian(s_capturedArgs.AsSpan(6));
        var typeName    = Encoding.UTF8.GetString(s_capturedArgs.AsSpan(8, typeNameLen));
        Assert.Contains("StringBuilder", typeName);
    }

    [Fact]
    public void InvokeDelegate_MultipleArgs_AllSerialised()
    {
        Bridge.CallJsCallback(42, [1, true, "x"]);

        Assert.Equal(3, s_capturedArgs[0]); // count
        Assert.Equal(0x03, s_capturedArgs[1]); // i32
        // skip 4 bytes of i32 value
        Assert.Equal(0x02, s_capturedArgs[6]); // bool true
        Assert.Equal(0x05, s_capturedArgs[7]); // string
    }

    [Fact]
    public unsafe void CallJsCallback_NoInvokerRegistered_DoesNotThrow()
    {
        Bridge.s_jsInvoker = null;
        var ex = Record.Exception(() => Bridge.CallJsCallback(99, [42]));
        Assert.Null(ex);
    }

    [Fact]
    public void InvokeDelegate_EventHandler_BothArgsSerialised()
    {
        const int id = 50;
        var handler = CreateDelegate<EventHandler>("System.EventHandler", id);
        var sender  = new object();
        var args    = EventArgs.Empty;

        handler(sender, args);

        Assert.Equal(id, s_capturedId);
        Assert.Equal(2, s_capturedArgs[0]); // two args

        // arg0: sender (object) → handle tag
        Assert.Equal(0x06, s_capturedArgs[1]);

        // arg1 offset: 1 (count) + 1 (tag) + 4 (handle id) + 2 (type len) + type_len
        var senderTypeLen = BinaryPrimitives.ReadUInt16LittleEndian(s_capturedArgs.AsSpan(6));
        int arg1Start     = 8 + senderTypeLen;
        Assert.Equal(0x06, s_capturedArgs[arg1Start]); // EventArgs → handle tag
    }

    // The native WinRT delegate path (NSWinRT.asDelegate / __nsAsDelegate) runs
    // through Rust's handle_as_delegate() in lib.rs, which uses MetadataReader
    // to look up the delegate GUID and parameter NativeTypes, then allocates a
    // JsDelegate COM object.
    //
    // Integration tests for the WinRT path live in the runtime integration test
    // suite where the full Windows Runtime metadata is available.  The C# bridge
    // side does not participate in the native WinRT path — there is no opcode for
    // it and no managed code involved.

    private static DispatchResult DispatchCreateDelegate(string typeName, int callbackId)
    {
        var pkt = BuildPacket(w =>
        {
            w.WriteByte(0x09);
            w.WriteString16(typeName);
            w.WriteI32(callbackId);
        });
        var r = new BinReader(pkt.AsSpan());
        return Bridge.DispatchBin(ref r);
    }

    private static T CreateDelegate<T>(string typeName, int callbackId) where T : Delegate
    {
        var result = DispatchCreateDelegate(typeName, callbackId);
        var handle = result.HandleId();
        Bridge.s_handles.TryGetValue(handle, out var obj);
        return (T)obj!;
    }

    private static void Release(DispatchResult result)
    {
        if (result.Kind() == DispatchKind.Handle)
            Release(result.HandleId());
    }

    private static void Release(int handle)
    {
        var pkt = BuildPacket(w => { w.WriteByte(0x04); w.WriteI32(handle); });
        var r   = new BinReader(pkt.AsSpan());
        Bridge.DispatchBin(ref r);
    }

    private static byte[] BuildPacket(Action<BinWriter> build)
    {
        var buf = new ArrayBufferWriter<byte>(64);
        var w   = new BinWriter(buf);
        build(w);
        return buf.WrittenSpan.ToArray();
    }
}
