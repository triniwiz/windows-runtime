using System;
using System.Buffers;
using System.Buffers.Binary;
using System.Collections.Generic;
using System.Text;
using NativeScriptBridge;
using Xunit;

namespace DotNetBridgeTests;

[Collection("Bridge")]
/// Tests the binary wire protocol: BinReader, BinWriter, DispatchResult.WriteAsBin,
/// and the binary dispatch path (Bridge.DispatchBin).
public sealed class BinaryProtocolTests : IDisposable
{
    public BinaryProtocolTests() => Bridge.ClearCaches();
    public void Dispose()        => Bridge.ClearCaches();

    // ── BinReader ─────────────────────────────────────────────────────────────

    [Fact]
    public void BinReader_ReadByte()
    {
        var r = Reader(0x01, 0x02);
        Assert.Equal(0x01, r.ReadByte());
        Assert.Equal(0x02, r.ReadByte());
    }

    [Fact]
    public void BinReader_ReadI32_LittleEndian()
    {
        var bytes = new byte[4];
        BinaryPrimitives.WriteInt32LittleEndian(bytes, -7);
        var r = Reader(bytes);
        Assert.Equal(-7, r.ReadI32());
    }

    [Fact]
    public void BinReader_ReadString16_Empty()
    {
        var r = Reader(0, 0);           // u16 len = 0
        Assert.Equal("", r.ReadString16());
    }

    [Fact]
    public void BinReader_ReadString16_Ascii()
    {
        var r = Reader(4, 0, (byte)'S', (byte)'t', (byte)'o', (byte)'p');
        Assert.Equal("Stop", r.ReadString16());
    }

    [Fact]
    public void BinReader_ReadString16_Utf8()
    {
        var s    = "café";
        var utf8 = Encoding.UTF8.GetBytes(s);
        var pkt  = new byte[2 + utf8.Length];
        BinaryPrimitives.WriteUInt16LittleEndian(pkt, (ushort)utf8.Length);
        utf8.CopyTo(pkt, 2);
        var r = Reader(pkt);
        Assert.Equal(s, r.ReadString16());
    }

    [Fact]
    public void BinReader_ReadArgs_Null()
    {
        var r = Reader(1, 0x00);        // count=1, tag=null
        var args = r.ReadArgs();
        Assert.Single(args);
        Assert.Null(args[0]);
    }

    [Fact]
    public void BinReader_ReadArgs_Booleans()
    {
        var r = Reader(2, 0x01, 0x02);  // false, true
        var args = r.ReadArgs();
        Assert.Equal(false, args[0]);
        Assert.Equal(true,  args[1]);
    }

    [Fact]
    public void BinReader_ReadArgs_I32()
    {
        var payload = new byte[6];
        payload[0] = 1;                 // count
        payload[1] = 0x03;              // i32 tag
        BinaryPrimitives.WriteInt32LittleEndian(payload.AsSpan(2), 42);
        var r = Reader(payload);
        var args = r.ReadArgs();
        Assert.Equal(42, args[0]);
    }

    [Fact]
    public void BinReader_ReadArgs_F64()
    {
        var payload = new byte[10];
        payload[0] = 1;                 // count
        payload[1] = 0x04;              // f64 tag
        BinaryPrimitives.WriteDoubleLittleEndian(payload.AsSpan(2), 3.14);
        var r = Reader(payload);
        var args = r.ReadArgs();
        Assert.Equal(3.14, (double)args[0]!, 10);
    }

    [Fact]
    public void BinReader_ReadArgs_String()
    {
        // count=1, tag=0x05, len=5, "hello"
        var r = Reader(1, 0x05, 5, 0, (byte)'h', (byte)'e', (byte)'l', (byte)'l', (byte)'o');
        var args = r.ReadArgs();
        Assert.Equal("hello", args[0]);
    }

    [Fact]
    public void BinReader_ReadArgs_HandleRef()
    {
        var payload = new byte[6];
        payload[0] = 1;                 // count
        payload[1] = 0x06;              // handle_ref tag
        BinaryPrimitives.WriteInt32LittleEndian(payload.AsSpan(2), 99);
        var r = Reader(payload);
        var args = r.ReadArgs();
        var hr = Assert.IsType<HandleRef>(args[0]);
        Assert.Equal(99, hr.Id);
    }

    // ── BinWriter ─────────────────────────────────────────────────────────────

    [Fact]
    public void BinWriter_WriteByte_Single()
    {
        var buf = new ArrayBufferWriter<byte>();
        var w   = new BinWriter(buf);
        w.WriteByte(0xAB);
        Assert.Equal(new byte[] { 0xAB }, buf.WrittenSpan.ToArray());
    }

    [Fact]
    public void BinWriter_WriteI32_LittleEndian()
    {
        var buf = new ArrayBufferWriter<byte>();
        var w   = new BinWriter(buf);
        w.WriteI32(-1);
        Assert.Equal(new byte[] { 0xFF, 0xFF, 0xFF, 0xFF }, buf.WrittenSpan.ToArray());
    }

    [Fact]
    public void BinWriter_WriteString16_Empty()
    {
        var buf = new ArrayBufferWriter<byte>();
        var w   = new BinWriter(buf);
        w.WriteString16("");
        Assert.Equal(new byte[] { 0, 0 }, buf.WrittenSpan.ToArray());
    }

    [Fact]
    public void BinWriter_WriteString32_Content()
    {
        var buf   = new ArrayBufferWriter<byte>();
        var w     = new BinWriter(buf);
        w.WriteString32("OK");
        var bytes = buf.WrittenSpan.ToArray();
        Assert.Equal(6, bytes.Length);
        Assert.Equal(2u, BinaryPrimitives.ReadUInt32LittleEndian(bytes));
        Assert.Equal((byte)'O', bytes[4]);
        Assert.Equal((byte)'K', bytes[5]);
    }

    // ── DispatchResult binary encoding ────────────────────────────────────────

    [Fact]
    public void WriteAsBin_Void_EmitsNullTag()
    {
        var buf = new ArrayBufferWriter<byte>();
        DispatchResult.Void.WriteAsBin(buf);
        Assert.Equal(new byte[] { 0x00 }, buf.WrittenSpan.ToArray());
    }

    [Fact]
    public void WriteAsBin_BoolFalse_EmitsFalseTag()
    {
        var buf = new ArrayBufferWriter<byte>();
        DispatchResult.Primitive(false, typeof(bool)).WriteAsBin(buf);
        Assert.Equal(new byte[] { 0x01 }, buf.WrittenSpan.ToArray());
    }

    [Fact]
    public void WriteAsBin_BoolTrue_EmitsTrueTag()
    {
        var buf = new ArrayBufferWriter<byte>();
        DispatchResult.Primitive(true, typeof(bool)).WriteAsBin(buf);
        Assert.Equal(new byte[] { 0x02 }, buf.WrittenSpan.ToArray());
    }

    [Fact]
    public void WriteAsBin_Int_EmitsI32Tag()
    {
        var buf = new ArrayBufferWriter<byte>();
        DispatchResult.Primitive(7, typeof(int)).WriteAsBin(buf);
        var bytes = buf.WrittenSpan.ToArray();
        Assert.Equal(0x03, bytes[0]);
        Assert.Equal(7, BinaryPrimitives.ReadInt32LittleEndian(bytes.AsSpan(1)));
    }

    [Fact]
    public void WriteAsBin_Double_EmitsF64Tag()
    {
        var buf = new ArrayBufferWriter<byte>();
        DispatchResult.Primitive(2.718, typeof(double)).WriteAsBin(buf);
        var bytes = buf.WrittenSpan.ToArray();
        Assert.Equal(0x04, bytes[0]);
        Assert.Equal(2.718, BinaryPrimitives.ReadDoubleLittleEndian(bytes.AsSpan(1)), 10);
    }

    [Fact]
    public void WriteAsBin_String_EmitsStringTag()
    {
        var buf = new ArrayBufferWriter<byte>();
        DispatchResult.Primitive("hi", typeof(string)).WriteAsBin(buf);
        var bytes = buf.WrittenSpan.ToArray();
        Assert.Equal(0x05, bytes[0]);
        var len = BinaryPrimitives.ReadUInt32LittleEndian(bytes.AsSpan(1));
        Assert.Equal(2u, len);
        Assert.Equal("hi", Encoding.UTF8.GetString(bytes, 5, 2));
    }

    [Fact]
    public void WriteAsBin_Handle_EmitsHandleTag()
    {
        var buf = new ArrayBufferWriter<byte>();
        DispatchResult.Handle(42, "System.Object").WriteAsBin(buf);
        var bytes = buf.WrittenSpan.ToArray();
        Assert.Equal(0x06, bytes[0]);
        Assert.Equal(42, BinaryPrimitives.ReadInt32LittleEndian(bytes.AsSpan(1)));
        var typeLen = BinaryPrimitives.ReadUInt16LittleEndian(bytes.AsSpan(5));
        Assert.Equal("System.Object", Encoding.UTF8.GetString(bytes, 7, typeLen));
    }

    [Fact]
    public void WriteAsBin_Array_EmitsArrayTag()
    {
        var buf = new ArrayBufferWriter<byte>();
        DispatchResult.Collection(new[] { 1, 2, 3 }).WriteAsBin(buf);
        var bytes = buf.WrittenSpan.ToArray();
        Assert.Equal(0x07, bytes[0]);
        Assert.Equal(3u, BinaryPrimitives.ReadUInt32LittleEndian(bytes.AsSpan(1)));
        // First element: i32 tag + value
        Assert.Equal(0x03, bytes[5]);
        Assert.Equal(1, BinaryPrimitives.ReadInt32LittleEndian(bytes.AsSpan(6)));
    }

    [Fact]
    public void WriteAsBin_Members_EmitsMembersTag()
    {
        var buf = new ArrayBufferWriter<byte>();
        DispatchResult.Members(["Foo"], ["Bar"], [], []).WriteAsBin(buf);
        var bytes = buf.WrittenSpan.ToArray();
        Assert.Equal(0x08, bytes[0]);
        // methods count = 1
        Assert.Equal(1, BinaryPrimitives.ReadUInt16LittleEndian(bytes.AsSpan(1)));
    }

    // ── binary dispatch (DispatchBin) ─────────────────────────────────────────

    [Fact]
    public void DispatchBin_Constructor_NoArgs_ReturnsHandle()
    {
        var pkt = BuildPacket(w =>
        {
            w.WriteByte(0x03);                           // ctor
            w.WriteString16("System.Text.StringBuilder");
            w.WriteString16("System");
            w.WriteByte(0);                              // arg_count
        });
        var r      = new BinReader(pkt.AsSpan());
        var result = Bridge.DispatchBin(ref r);
        Assert.Equal(DispatchKind.Handle, result.Kind());
        Bridge.Dispatch(new InvokeRequest(null, null, "__release", result.HandleId(), null));
    }

    [Fact]
    public void DispatchBin_InstanceMethodCall_LengthAfterAppend()
    {
        // Create via JSON path, then dispatch via binary
        var createReq = new InvokeRequest("System", "System.Text.StringBuilder", ".ctor", null, null);
        var handle    = Bridge.Dispatch(createReq).HandleId();

        var appendPkt = BuildPacket(w =>
        {
            w.WriteByte(0x01);           // instance call
            w.WriteI32(handle);
            w.WriteString16("Append");
            w.WriteByte(1);              // arg_count
            w.WriteByte(0x05);           // string arg
            w.WriteString16("hello");
        });
        var r = new BinReader(appendPkt.AsSpan());
        Bridge.DispatchBin(ref r);

        var lengthPkt = BuildPacket(w =>
        {
            w.WriteByte(0x01);           // instance call
            w.WriteI32(handle);
            w.WriteString16("get_Length");
            w.WriteByte(0);
        });
        var r2     = new BinReader(lengthPkt.AsSpan());
        var length = Bridge.DispatchBin(ref r2);
        Assert.Equal(DispatchKind.Primitive, length.Kind());
        Assert.Equal(5, Convert.ToInt32(length.PrimitiveValue()));

        Bridge.Dispatch(new InvokeRequest(null, null, "__release", handle, null));
    }

    [Fact]
    public void DispatchBin_Release_RemovesHandle()
    {
        var createReq = new InvokeRequest("System", "System.Text.StringBuilder", ".ctor", null, null);
        var handle    = Bridge.Dispatch(createReq).HandleId();

        var relPkt = BuildPacket(w =>
        {
            w.WriteByte(0x04);           // release
            w.WriteI32(handle);
        });
        var r = new BinReader(relPkt.AsSpan());
        Bridge.DispatchBin(ref r);

        // Accessing the handle after release should throw.
        // ref struct can't be captured in a lambda, so catch manually.
        var threw = false;
        try
        {
            var r2 = new BinReader(BuildPacket(w =>
            {
                w.WriteByte(0x01); w.WriteI32(handle);
                w.WriteString16("get_Length"); w.WriteByte(0);
            }).AsSpan());
            Bridge.DispatchBin(ref r2);
        }
        catch (KeyNotFoundException) { threw = true; }
        Assert.True(threw, "expected KeyNotFoundException for released handle");
    }

    [Fact]
    public void DispatchBin_StaticCall_MathAbs()
    {
        var pkt = BuildPacket(w =>
        {
            w.WriteByte(0x02);           // static call
            w.WriteString16("System.Math");
            w.WriteString16("System");
            w.WriteString16("Abs");
            w.WriteByte(1);              // arg_count
            w.WriteByte(0x03);           // i32
            w.WriteI32(-7);
        });
        var r      = new BinReader(pkt.AsSpan());
        var result = Bridge.DispatchBin(ref r);
        Assert.Equal(DispatchKind.Primitive, result.Kind());
        Assert.Equal(7, Convert.ToInt32(result.PrimitiveValue()));
    }

    [Fact]
    public void DispatchBin_Members_ByType_ReturnsMembersTag()
    {
        var pkt = BuildPacket(w =>
        {
            w.WriteByte(0x06);           // members by type
            w.WriteString16("System.Math");
            w.WriteString16("System");
        });
        var r      = new BinReader(pkt.AsSpan());
        var result = Bridge.DispatchBin(ref r);
        Assert.Equal(DispatchKind.Members, result.Kind());
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    private static BinReader Reader(params byte[] bytes) =>
        new BinReader(bytes.AsSpan());

    private static byte[] BuildPacket(Action<BinWriter> build)
    {
        var buf = new ArrayBufferWriter<byte>(64);
        var w   = new BinWriter(buf);
        build(w);
        return buf.WrittenSpan.ToArray();
    }
}
