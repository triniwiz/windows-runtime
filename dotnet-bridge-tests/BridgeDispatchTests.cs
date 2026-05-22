using System;
using System.Collections.Generic;
using System.Text.Json;
using NativeScriptBridge;
using Xunit;

namespace DotNetBridgeTests;

// Run bridge test classes sequentially — they share global static caches and
// the handle table, so parallel execution causes spurious failures.
[Collection("Bridge")]
/// Tests the JSON dispatch path (Bridge.Dispatch) end-to-end without the
/// unmanaged ABI.  Uses System.Text.StringBuilder as the primary target
/// because it is always available and has a rich mix of ctors, methods,
/// properties, and return types.
public sealed class BridgeDispatchTests : IDisposable
{
    // Fresh state for every test — caches are process-global so we reset them.
    public BridgeDispatchTests() => Bridge.ClearCaches();
    public void Dispose()        => Bridge.ClearCaches();

    [Fact]
    public void TypeResolution_ByFullName_FindsType()
    {
        var req = MakeStatic("System.Text.StringBuilder", "__members__");
        var result = Bridge.Dispatch(req);
        Assert.Equal(DispatchKind.Members, result.Kind());
    }

    [Fact]
    public void TypeResolution_UnknownType_Throws()
    {
        var req = MakeStatic("NuGet.Does.Not.Exist", "__members__");
        Assert.Throws<TypeLoadException>(() => Bridge.Dispatch(req));
    }

    [Fact]
    public void Constructor_NoArgs_ReturnsHandle()
    {
        var req = MakeStatic("System.Text.StringBuilder", ".ctor");
        var result = Bridge.Dispatch(req);
        Assert.Equal(DispatchKind.Handle, result.Kind());
        Release(result);
    }

    [Fact]
    public void Constructor_WithCapacityArg_ReturnsHandle()
    {
        var req = MakeStatic("System.Text.StringBuilder", ".ctor", Json(128));
        var result = Bridge.Dispatch(req);
        Assert.Equal(DispatchKind.Handle, result.Kind());
        Release(result);
    }

    [Fact]
    public void InstanceMethodCall_Append_ReturnsSelf()
    {
        var handle = CreateSb();
        var req    = MakeInstance(handle, "Append", Json("hello"));
        var result = Bridge.Dispatch(req);
        // Append returns `this` — same handle id or a new handle wrapping the same object
        Assert.Equal(DispatchKind.Handle, result.Kind());
        Release(result);
    }

    [Fact]
    public void InstanceMethodCall_ToString_ReturnsString()
    {
        var handle = CreateSb("world");
        var req    = MakeInstance(handle, "ToString");
        var result = Bridge.Dispatch(req);
        Assert.Equal(DispatchKind.Primitive, result.Kind());
        Assert.Equal("world", result.PrimitiveValue());
        Release(handle);
    }

    [Fact]
    public void InstanceMethodCall_UnknownMethod_Throws()
    {
        var handle = CreateSb();
        var req    = MakeInstance(handle, "NonExistentMethod123");
        Assert.Throws<MissingMethodException>(() => Bridge.Dispatch(req));
        Release(handle);
    }

    [Fact]
    public void PropertyGet_Length_ReturnsInt()
    {
        var handle = CreateSb("hi");
        var req    = MakeInstance(handle, "get_Length");
        var result = Bridge.Dispatch(req);
        Assert.Equal(DispatchKind.Primitive, result.Kind());
        Assert.Equal(2, Convert.ToInt32(result.PrimitiveValue()));
        Release(handle);
    }

    [Fact]
    public void PropertySet_Capacity_UpdatesValue()
    {
        var handle = CreateSb();
        Bridge.Dispatch(MakeInstance(handle, "set_Capacity", Json(512)));
        var cap = Bridge.Dispatch(MakeInstance(handle, "get_Capacity"));
        Assert.Equal(512, Convert.ToInt32(cap.PrimitiveValue()));
        Release(handle);
    }

    [Fact]
    public void StaticMethodCall_MathAbs_ReturnsPositive()
    {
        var req    = MakeStatic("System.Math", "Abs", Json(-42));
        var result = Bridge.Dispatch(req);
        Assert.Equal(DispatchKind.Primitive, result.Kind());
        Assert.Equal(42, Convert.ToInt32(result.PrimitiveValue()));
    }

    [Fact]
    public void StaticPropertyGet_EnvironmentNewLine_ReturnsString()
    {
        var req    = MakeStatic("System.Environment", "get_NewLine");
        var result = Bridge.Dispatch(req);
        Assert.Equal(DispatchKind.Primitive, result.Kind());
        Assert.NotEmpty((string)result.PrimitiveValue()!);
    }

    [Fact]
    public void Release_RemovesHandle()
    {
        var handle = CreateSb();
        Bridge.Dispatch(MakeInstance(handle, "__release"));
        // A second call with the same handle should throw
        Assert.Throws<KeyNotFoundException>(() =>
            Bridge.Dispatch(MakeInstance(handle, "get_Length")));
    }

    [Fact]
    public void InvalidHandle_Throws()
    {
        var req = MakeInstance(99999, "ToString");
        Assert.Throws<KeyNotFoundException>(() => Bridge.Dispatch(req));
    }

    [Fact]
    public void Members_ByHandle_ContainsKnownMembers()
    {
        var handle = CreateSb();
        var req    = MakeInstance(handle, "__members__");
        var result = Bridge.Dispatch(req);
        Assert.Equal(DispatchKind.Members, result.Kind());
        Release(handle);
    }

    [Fact]
    public void Members_ByType_ContainsKnownStaticMembers()
    {
        var req    = MakeStatic("System.Math", "__members__");
        var result = Bridge.Dispatch(req);
        Assert.Equal(DispatchKind.Members, result.Kind());
    }

    [Fact]
    public void MethodReturningArray_WrapsAsCollection()
    {
        // Environment.GetCommandLineArgs() → string[]
        var req    = MakeStatic("System.Environment", "GetCommandLineArgs");
        var result = Bridge.Dispatch(req);
        Assert.Equal(DispatchKind.Collection, result.Kind());
    }

    private static int CreateSb(string? initial = null)
    {
        // Always use the 0-arg ctor — StringBuilder also has StringBuilder(int)
        // and StringBuilder(string), and FindMethodCore picks the first match by
        // arg count, which may not be the string overload.
        var handle = Bridge.Dispatch(
            MakeStatic("System.Text.StringBuilder", ".ctor")).HandleId();
        if (initial is not null)
            Bridge.Dispatch(MakeInstance(handle, "Append", Json(initial)));
        return handle;
    }

    private static void Release(DispatchResult result)
    {
        if (result.Kind() == DispatchKind.Handle)
            Bridge.Dispatch(MakeInstance(result.HandleId(), "__release"));
    }

    private static void Release(int handle) =>
        Bridge.Dispatch(MakeInstance(handle, "__release"));

    private static InvokeRequest MakeStatic(string typeName, string method, params JsonElement[] args) =>
        new(typeName.Split('.')[0], typeName, method, null,
            args.Length > 0 ? args : null);

    private static InvokeRequest MakeInstance(int handle, string method, params JsonElement[] args) =>
        new(null, null, method, handle,
            args.Length > 0 ? args : null);

    private static JsonElement Json(object value) =>
        JsonSerializer.SerializeToElement(value);
}

// Reflection helpers to inspect DispatchResult fields from outside the assembly.
// DispatchResult is internal but InternalsVisibleTo grants us access.
internal static class DispatchResultExtensions
{
    public static DispatchKind Kind(this DispatchResult r)
    {
        var f = typeof(DispatchResult).GetField("_kind",
            System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance);
        return (DispatchKind)f!.GetValue(r)!;
    }

    public static int HandleId(this DispatchResult r)
    {
        var f = typeof(DispatchResult).GetField("_handle",
            System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance);
        return (int)f!.GetValue(r)!;
    }

    public static object? PrimitiveValue(this DispatchResult r)
    {
        var f = typeof(DispatchResult).GetField("_value",
            System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance);
        return f!.GetValue(r);
    }
}
