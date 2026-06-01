// dotnet-typings-gen — emit TypeScript declarations from a .NET DLL or .csproj.
//
// Usage:
//   dotnet run --project dotnet-typings-gen.csproj -- \
//       --input  NativeScript.Widgets.dll \
//       --root   NativeScript             \
//       --out    NativeScript.Widgets.d.ts
//
// --input  Path to a .dll or .csproj (builds the project first if .csproj).
// --root   Namespace prefix to include (optional — all public types if omitted).
// --out    Output .d.ts file path.

using System.Collections.Immutable;
using System.Diagnostics;
using System.Reflection;
using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;
using System.Text;

return Generator.Run(args);

// ═══════════════════════════════════════════════════════════════════════════════

static class Generator
{
    public static int Run(string[] args)
    {
        string? outPath = null;
        var inputs = new List<string>();
        var roots = new List<string>();
        for (int i = 0; i < args.Length; i++)
            switch (args[i])
            {
                // --input may be passed multiple times; all are read and merged (e.g. the Windows App
                // SDK splits Microsoft.UI.* across Microsoft.WinUI.dll and the InteractiveExperiences
                // projection).
                case "--input": inputs.Add(args[++i]); break;
                case "--inputs": inputs.AddRange(args[++i].Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)); break;
                case "--out":   outPath   = args[++i]; break;
                // --root may be passed multiple times; a type is included if it matches ANY root.
                case "--root":  roots.Add(args[++i]); break;
                case "--roots": roots.AddRange(args[++i].Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)); break;
            }

        if (inputs.Count == 0 || outPath == null)
        {
            Console.Error.WriteLine("Usage: dotnet-typings-gen --input <dll|csproj> [--input <dll> ...] [--root <ns> ...] --out <file>");
            return 1;
        }

        var modules = new SortedDictionary<string, List<string>>(StringComparer.Ordinal);
        // Track (namespace + decl-name) so the same type appearing in multiple inputs isn't emitted twice.
        var seenTypes = new HashSet<string>(StringComparer.Ordinal);

        // Resolve every input to a concrete assembly path once (building .csproj inputs as needed).
        var resolvedInputs = new List<string>();
        foreach (var rawInput in inputs)
        {
            var inputPath = rawInput;
            if (inputPath.EndsWith(".csproj", StringComparison.OrdinalIgnoreCase))
            {
                inputPath = BuildCsProj(inputPath);
                if (inputPath == null) return 1;
            }
            if (!File.Exists(inputPath))
            {
                Console.Error.WriteLine($"[typings-gen] Input not found: {inputPath}");
                return 1;
            }
            resolvedInputs.Add(inputPath);
        }

        // Pre-pass: collect every enum's full name across ALL inputs so the decoder can render
        // enum-typed members as `number` even when the enum lives in a different winmd than the user.
        foreach (var inputPath in resolvedInputs)
        {
            using var efs = File.OpenRead(inputPath);
            using var epe = new PEReader(efs);
            var er = epe.GetMetadataReader();
            foreach (var th in er.TypeDefinitions)
            {
                var td = er.GetTypeDefinition(th);
                if (td.BaseType.IsNil) continue;
                if (Helpers.ResolveEntityName(er, td.BaseType) != "System.Enum") continue;
                TsTypeDecoder.EnumNames.Add(
                    Helpers.StripArity(Helpers.FullName(er.GetString(td.Namespace), er.GetString(td.Name))));
            }
        }

        foreach (var inputPath in resolvedInputs)
        {
            using var fs     = File.OpenRead(inputPath);
            using var pe     = new PEReader(fs);
            var       reader = pe.GetMetadataReader();
            var       dec    = new TsTypeDecoder(reader);

            // full-name → TypeDef map so interface TypeReferences within this winmd can be resolved and
            // walked (needed to reach base collection interfaces like IVector for member inlining).
            var nameToDef = new Dictionary<string, TypeDefinitionHandle>(StringComparer.Ordinal);
            foreach (var th in reader.TypeDefinitions)
            {
                var td = reader.GetTypeDefinition(th);
                nameToDef[Helpers.FullName(reader.GetString(td.Namespace), reader.GetString(td.Name))] = th;
            }

            foreach (var typeHandle in reader.TypeDefinitions)
            {
                var typeDef = reader.GetTypeDefinition(typeHandle);
                var vis     = typeDef.Attributes & TypeAttributes.VisibilityMask;
                if (vis != TypeAttributes.Public && vis != TypeAttributes.NestedPublic) continue;

                var ns       = reader.GetString(typeDef.Namespace);
                var name     = Helpers.StripArity(reader.GetString(typeDef.Name));
                var fullName = Helpers.FullName(ns, name);

                // Skip CsWinRT-internal scaffolding (ABI projections, WinRT runtime helpers, compiler-
                // generated types) — these are implementation detail, not part of the public surface.
                if (ns.StartsWith("ABI.", StringComparison.Ordinal)
                    || ns == "ABI"
                    || ns.StartsWith("WinRT", StringComparison.Ordinal)
                    || name.StartsWith("<", StringComparison.Ordinal)) continue;

                // Include if no root filter, or the type matches ANY requested root.
                if (roots.Count > 0
                    && !roots.Any(r => fullName.Equals(r, StringComparison.Ordinal)
                                       || fullName.StartsWith(r + ".", StringComparison.Ordinal))) continue;

                if (!seenTypes.Add(fullName)) continue; // already emitted from an earlier input

                var decl = EmitTypeDecl(reader, dec, nameToDef, typeDef, name);
                if (decl == null) continue;

                if (!modules.TryGetValue(ns, out var list))
                    modules[ns] = list = [];
                list.Add(decl);
            }
        }

        var sb = new StringBuilder();
        sb.AppendLine("// Auto-generated by typings-generator (dotnet mode).");
        sb.AppendLine("// Do not edit - re-run build.windows.ps1 to regenerate.");
        sb.AppendLine();
        // Ambient (script-style) declarations to match windows.d.ts so this file can be wired the same
        // way via a /// <reference path> in references.d.ts (no `export`/`declare global` wrapper).
        foreach (var (ns, decls) in modules)
        {
            sb.AppendLine($"declare namespace {ns} {{");
            foreach (var d in decls) sb.AppendLine(d);
            sb.AppendLine("}");
            sb.AppendLine();
        }

        var dir = Path.GetDirectoryName(outPath);
        if (!string.IsNullOrEmpty(dir)) Directory.CreateDirectory(dir);
        File.WriteAllText(outPath, sb.ToString(), new UTF8Encoding(encoderShouldEmitUTF8Identifier: false));

        int total = modules.Values.Sum(v => v.Count);
        Console.WriteLine($"[typings-gen] {total} type(s) → {outPath}");
        return 0;
    }

    static string? BuildCsProj(string csprojPath)
    {
        var psi = new ProcessStartInfo("dotnet",
            $"build \"{csprojPath}\" -c Release --verbosity minimal --nologo")
        {
            RedirectStandardOutput = true,
            RedirectStandardError  = true,
            UseShellExecute        = false,
        };
        using var proc = Process.Start(psi)!;
        string stdout = proc.StandardOutput.ReadToEnd();
        string stderr = proc.StandardError.ReadToEnd();
        proc.WaitForExit();

        if (proc.ExitCode != 0)
        {
            Console.Error.WriteLine($"[typings-gen] Build failed:\n{stderr}");
            return null;
        }

        foreach (var line in stdout.Split('\n'))
        {
            int arrow = line.IndexOf("->", StringComparison.Ordinal);
            if (arrow < 0) continue;
            var dll = line[(arrow + 2)..].Trim();
            if (dll.EndsWith(".dll", StringComparison.OrdinalIgnoreCase) && File.Exists(dll))
                return dll;
        }

        Console.Error.WriteLine("[typings-gen] Could not locate output DLL in build log.");
        return null;
    }

    static string? EmitTypeDecl(MetadataReader reader, TsTypeDecoder dec, Dictionary<string, TypeDefinitionHandle> nameToDef,
                                 TypeDefinition typeDef, string simpleName)
    {
        bool isIface = (typeDef.Attributes & TypeAttributes.Interface) != 0;
        string kw    = isIface ? "interface" : "class";

        // WinRT delegates: the winmd .ctor is (object, native int), but JS constructs them with a single
        // callback — `new RoutedEventHandler(fn)`. Emit a 1-arg callback constructor derived from Invoke
        // (matches windows.d.ts and how the runtime wraps the JS function).
        if (!isIface && !typeDef.BaseType.IsNil)
        {
            string bn = Helpers.ResolveEntityName(reader, typeDef.BaseType);
            if (bn == "System.MulticastDelegate" || bn == "System.Delegate")
            {
                foreach (var mh in typeDef.GetMethods())
                {
                    var m = reader.GetMethodDefinition(mh);
                    if (reader.GetString(m.Name) != "Invoke") continue;
                    MethodSignature<string> isig;
                    try { isig = m.DecodeSignature(dec, null); } catch { return null; }
                    var ph = m.GetParameters().ToArray();
                    var ps = new List<string>();
                    for (int i = 0; i < isig.ParameterTypes.Length; i++)
                    {
                        string pn = "p" + i;
                        if (i < ph.Length) { var cn = reader.GetString(reader.GetParameter(ph[i]).Name); if (!string.IsNullOrWhiteSpace(cn)) pn = cn; }
                        ps.Add($"{pn}: {isig.ParameterTypes[i]}");
                    }
                    string cb = $"({string.Join(", ", ps)}) => {isig.ReturnType}";
                    var dsb = new StringBuilder();
                    dsb.AppendLine($"    class {simpleName} {{");
                    dsb.AppendLine($"      constructor(handler: {cb});");
                    dsb.AppendLine($"      Invoke({string.Join(", ", ps)}): {isig.ReturnType};");
                    dsb.Append("    }");
                    return dsb.ToString();
                }
                return null;
            }
        }

        string extendsClause = "";
        if (!isIface && !typeDef.BaseType.IsNil)
        {
            string baseName = Helpers.ResolveEntityName(reader, typeDef.BaseType);
            if (!string.IsNullOrEmpty(baseName) && baseName != "System.Object")
                extendsClause = $" extends {Helpers.MapBuiltinType(baseName)}";
        }

        // Implemented interfaces — emit them so inherited WinRT members (IVector.Append/Size, IMap.Insert,
        // etc.) resolve against the interface declarations (mostly in windows.d.ts / Windows.Foundation).
        // A class uses `implements`; an interface uses `extends` (and has no base-type extends clause).
        var ifaces = new List<string>();
        foreach (var iiHandle in typeDef.GetInterfaceImplementations())
        {
            var ii = reader.GetInterfaceImplementation(iiHandle);
            string iname = Helpers.ResolveTypeName(reader, dec, ii.Interface);
            if (string.IsNullOrEmpty(iname) || iname.StartsWith("System.", StringComparison.Ordinal)
                || iname.StartsWith("WinRT", StringComparison.Ordinal)) continue;
            ifaces.Add(Helpers.MapBuiltinType(iname));
        }
        ifaces = ifaces.Distinct().ToList();
        string implementsClause = ifaces.Count == 0 ? ""
            : (isIface ? (string.IsNullOrEmpty(extendsClause) ? " extends " : ", ") : " implements ") + string.Join(", ", ifaces);

        var sb = new StringBuilder();
        sb.AppendLine($"    {kw} {simpleName}{extendsClause}{implementsClause} {{");

        // Member names emitted on THIS type, so inlined collection members below don't duplicate them.
        var emitted = new HashSet<string>(StringComparer.Ordinal);
        var accessors = CollectPropertyAccessors(reader, typeDef);

        // WinRT events → assignable `Name: Handler | null` properties (matches windows.d.ts and how JS
        // assigns handlers). Their add_/remove_ accessor methods are collected so the method loop skips them.
        var eventAccessors = new HashSet<MethodDefinitionHandle>();
        foreach (var evHandle in typeDef.GetEvents())
        {
            var ev = reader.GetEventDefinition(evHandle);
            var eacc = ev.GetAccessors();
            if (!eacc.Adder.IsNil) eventAccessors.Add(eacc.Adder);
            if (!eacc.Remover.IsNil) eventAccessors.Add(eacc.Remover);
            if (!eacc.Raiser.IsNil) eventAccessors.Add(eacc.Raiser);
            if (eacc.Adder.IsNil) continue;
            var adder = reader.GetMethodDefinition(eacc.Adder);
            if ((adder.Attributes & MethodAttributes.MemberAccessMask) != MethodAttributes.Public) continue;
            bool evStatic = (adder.Attributes & MethodAttributes.Static) != 0;
            string evName = reader.GetString(ev.Name);
            string handler = Helpers.MapBuiltinType(Helpers.ResolveTypeName(reader, dec, ev.Type));
            sb.AppendLine($"      {(evStatic ? "static " : "")}{evName}: {handler} | null;");
            emitted.Add(evName);
        }

        // Public fields (e.g. static readonly DependencyProperty FooProperty)
        foreach (var fieldHandle in typeDef.GetFields())
        {
            var field = reader.GetFieldDefinition(fieldHandle);
            if ((field.Attributes & FieldAttributes.FieldAccessMask) != FieldAttributes.Public) continue;

            string fName  = reader.GetString(field.Name);
            bool   fStat  = (field.Attributes & FieldAttributes.Static) != 0;
            bool   fRo    = (field.Attributes & FieldAttributes.InitOnly) != 0;
            string fType;
            try   { fType = field.DecodeSignature(dec, null); }
            catch { continue; }

            sb.AppendLine($"      {(fStat ? "static " : "")}{(fRo ? "readonly " : "")}{fName}: {fType};");
            emitted.Add(fName);
        }

        // Public properties
        foreach (var propHandle in typeDef.GetProperties())
        {
            var prop = reader.GetPropertyDefinition(propHandle);
            var acc  = prop.GetAccessors();
            if (acc.Getter.IsNil) continue;

            var getter = reader.GetMethodDefinition(acc.Getter);
            // Skip non-public properties (e.g. private helper properties)
            if ((getter.Attributes & MethodAttributes.MemberAccessMask) != MethodAttributes.Public) continue;

            bool stat    = (getter.Attributes & MethodAttributes.Static) != 0;
            bool ro      = acc.Setter.IsNil;
            string pName = reader.GetString(prop.Name);
            MethodSignature<string> sig;
            try   { sig = prop.DecodeSignature(dec, null); }
            catch { continue; }

            sb.AppendLine($"      {(stat ? "static " : "")}{(ro ? "readonly " : "")}{pName}: {sig.ReturnType};");
            emitted.Add(pName);
        }

        foreach (var methodHandle in typeDef.GetMethods())
        {
            if (accessors.Contains(methodHandle) || eventAccessors.Contains(methodHandle)) continue;
            var method = reader.GetMethodDefinition(methodHandle);
            if ((method.Attributes & MethodAttributes.MemberAccessMask) != MethodAttributes.Public) continue;

            string mName = reader.GetString(method.Name);
            bool isCtor = mName == ".ctor";
            if (mName.StartsWith('.') && !isCtor) continue; // skip .cctor; .ctor → constructor below

            bool isStatic = (method.Attributes & MethodAttributes.Static) != 0;
            MethodSignature<string> sig;
            try   { sig = method.DecodeSignature(dec, null); }
            catch { continue; }

            var paramHandles = method.GetParameters().ToArray();
            var paramStrs    = new List<string>();
            for (int i = 0; i < sig.ParameterTypes.Length; i++)
            {
                string pn = "p" + i;
                if (i < paramHandles.Length)
                {
                    var candidate = reader.GetString(reader.GetParameter(paramHandles[i]).Name);
                    if (!string.IsNullOrWhiteSpace(candidate)) pn = candidate;
                }
                paramStrs.Add($"{pn}: {sig.ParameterTypes[i]}");
            }

            if (isCtor)
                sb.AppendLine($"      constructor({string.Join(", ", paramStrs)});");
            else
            {
                sb.AppendLine($"      {(isStatic ? "static " : "")}{mName}({string.Join(", ", paramStrs)}): {sig.ReturnType};");
                emitted.Add(mName);
            }
        }

        // WinRT collection runtimeclasses (UIElementCollection, ResourceDictionary, *Collection…) get their
        // members from IVector/IMap/IIterable, which they implement transitively. TS `implements` does NOT
        // copy members into the class, and those interfaces live in Windows.Foundation (a different .winmd),
        // so inline the canonical members here when the type implements a collection interface (deduped).
        if (!isIface)
        {
            var allIfaces = new HashSet<string>(StringComparer.Ordinal);
            CollectAllInterfaceNames(reader, dec, typeDef, nameToDef, allIfaces, new HashSet<int>());
            // Detect by interface graph, plus a WinRT-naming heuristic: *Collection ⇒ IVector (Append/
            // Size/GetAt/InsertAt…), *Dictionary ⇒ IMap (Insert/Remove/Lookup…). The recursion can miss
            // an indirect `requires IVector` when the interface is a cross-winmd ref; the name heuristic
            // reliably covers the WinUI collection/dictionary runtimeclasses core uses.
            bool vector = simpleName.EndsWith("Collection", StringComparison.Ordinal)
                || allIfaces.Any(n => n.Contains(".IVector<") || n.Contains(".IVectorView<") || n.Contains(".IObservableVector<") || n.Contains(".IBindableVector"));
            bool map    = simpleName.EndsWith("Dictionary", StringComparison.Ordinal)
                || allIfaces.Any(n => n.Contains(".IMap<") || n.Contains(".IMapView<") || n.Contains(".IObservableMap<") || n.Contains(".IBindableMap"));
            bool iter   = allIfaces.Any(n => n.Contains(".IIterable<"));
            void Add(string member, string decl) { if (emitted.Add(member)) sb.AppendLine("      " + decl); }
            if (vector)
            {
                Add("GetAt", "GetAt(index: number): any;");
                Add("Size", "readonly Size: number;");
                Add("IndexOf", "IndexOf(value: any, index: number): boolean;");
                Add("SetAt", "SetAt(index: number, value: any): void;");
                Add("InsertAt", "InsertAt(index: number, value: any): void;");
                Add("RemoveAt", "RemoveAt(index: number): void;");
                Add("Append", "Append(value: any): void;");
                Add("RemoveAtEnd", "RemoveAtEnd(): void;");
                Add("Clear", "Clear(): void;");
                Add("GetMany", "GetMany(startIndex: number, items: any[]): number;");
                Add("ReplaceAll", "ReplaceAll(items: any[]): void;");
                Add("GetView", "GetView(): any;");
            }
            if (map)
            {
                Add("Lookup", "Lookup(key: any): any;");
                Add("Size", "readonly Size: number;");
                Add("HasKey", "HasKey(key: any): boolean;");
                Add("Insert", "Insert(key: any, value: any): boolean;");
                Add("Remove", "Remove(key: any): void;");
                Add("Clear", "Clear(): void;");
                Add("GetView", "GetView(): any;");
            }
            if (vector || map || iter) Add("First", "First(): any;");
        }

        sb.Append("    }");
        return sb.ToString();
    }

    // Collects the decoded names of all interfaces a type implements, recursing through interfaces that
    // are TypeDefinitions in the SAME .winmd (so a class → IUIElementCollection → IVector<…> chain is seen).
    static void CollectAllInterfaceNames(MetadataReader reader, TsTypeDecoder dec, TypeDefinition typeDef,
                                         Dictionary<string, TypeDefinitionHandle> nameToDef, HashSet<string> acc, HashSet<int> visited)
    {
        foreach (var iiHandle in typeDef.GetInterfaceImplementations())
        {
            var h = reader.GetInterfaceImplementation(iiHandle).Interface;
            var nm = Helpers.ResolveTypeName(reader, dec, h);
            if (!string.IsNullOrEmpty(nm)) acc.Add(nm);

            // Resolve to a local TypeDef and recurse so a class → IUIElementCollection → IVector<…> chain
            // is fully walked. Same-winmd interfaces may appear as TypeDefinition OR TypeReference.
            TypeDefinitionHandle? defH = null;
            if (h.Kind == HandleKind.TypeDefinition) defH = (TypeDefinitionHandle)h;
            else if (h.Kind == HandleKind.TypeReference)
            {
                var refName = Helpers.ResolveEntityName(reader, h);
                if (nameToDef.TryGetValue(refName, out var found)) defH = found;
            }
            if (defH.HasValue)
            {
                var tok = System.Reflection.Metadata.Ecma335.MetadataTokens.GetToken(defH.Value);
                if (visited.Add(tok))
                    CollectAllInterfaceNames(reader, dec, reader.GetTypeDefinition(defH.Value), nameToDef, acc, visited);
            }
        }
    }

    static HashSet<MethodDefinitionHandle> CollectPropertyAccessors(MetadataReader reader, TypeDefinition typeDef)
    {
        var set = new HashSet<MethodDefinitionHandle>();
        foreach (var propHandle in typeDef.GetProperties())
        {
            var acc = reader.GetPropertyDefinition(propHandle).GetAccessors();
            if (!acc.Getter.IsNil) set.Add(acc.Getter);
            if (!acc.Setter.IsNil) set.Add(acc.Setter);
        }
        return set;
    }
}

// ═══════════════════════════════════════════════════════════════════════════════

static class Helpers
{
    // CLR encodes generic type names with a backtick-arity suffix (e.g. `IAsyncOperation`1`,
    // `TypedEventHandler`2`). That backtick is invalid in TypeScript (it starts a template literal),
    // so strip it everywhere a type name is rendered — the generic <…> args are appended separately.
    public static string StripArity(string name)
    {
        int tick = name.IndexOf('`');
        return tick >= 0 ? name.Substring(0, tick) : name;
    }

    public static string FullName(string ns, string name)
    {
        name = StripArity(name);
        return string.IsNullOrEmpty(ns) ? name : $"{ns}.{name}";
    }

    // Resolve a type name from an entity handle, decoding generic instantiations (TypeSpecification)
    // like IVector`1<UIElement> via the signature provider; plain TypeDef/TypeRef via ResolveEntityName.
    public static string ResolveTypeName(MetadataReader reader, TsTypeDecoder dec, EntityHandle handle)
    {
        if (handle.IsNil) return string.Empty;
        if (handle.Kind == HandleKind.TypeSpecification)
        {
            try { return reader.GetTypeSpecification((TypeSpecificationHandle)handle).DecodeSignature(dec, null); }
            catch { return string.Empty; }
        }
        return ResolveEntityName(reader, handle);
    }

    public static string ResolveEntityName(MetadataReader reader, EntityHandle handle)
    {
        if (handle.IsNil) return string.Empty;
        if (handle.Kind == HandleKind.TypeDefinition)
        {
            var td = reader.GetTypeDefinition((TypeDefinitionHandle)handle);
            return FullName(reader.GetString(td.Namespace), reader.GetString(td.Name));
        }
        if (handle.Kind == HandleKind.TypeReference)
        {
            var tr = reader.GetTypeReference((TypeReferenceHandle)handle);
            return FullName(reader.GetString(tr.Namespace), reader.GetString(tr.Name));
        }
        return string.Empty;
    }

    public static string MapBuiltinType(string name) => name switch
    {
        "System.Boolean"               => "boolean",
        "System.String" or "System.Char" => "string",
        "System.Byte"   or "System.SByte"
            or "System.Int16"  or "System.UInt16"
            or "System.Int32"  or "System.UInt32"
            or "System.Int64"  or "System.UInt64"
            or "System.Single" or "System.Double"
            or "System.IntPtr" or "System.UIntPtr" => "number",
        "System.Object" => "any",
        "System.Void"   => "void",
        _               => name,
    };
}

// ═══════════════════════════════════════════════════════════════════════════════

class TsTypeDecoder : ISignatureTypeProvider<string, object?>
{
    private readonly MetadataReader _r;
    public TsTypeDecoder(MetadataReader r) => _r = r;

    // Full names of every enum across all input winmds. Enum-typed members are emitted as `number`
    // (matching windows.d.ts) so callers can assign raw numbers or enum members interchangeably.
    public static readonly HashSet<string> EnumNames = new HashSet<string>(StringComparer.Ordinal);

    public string GetPrimitiveType(PrimitiveTypeCode code) => code switch
    {
        PrimitiveTypeCode.Void    => "void",
        PrimitiveTypeCode.Boolean => "boolean",
        PrimitiveTypeCode.Char    => "string",
        PrimitiveTypeCode.String  => "string",
        PrimitiveTypeCode.Object  => "any",
        PrimitiveTypeCode.SByte   or PrimitiveTypeCode.Byte
            or PrimitiveTypeCode.Int16  or PrimitiveTypeCode.UInt16
            or PrimitiveTypeCode.Int32  or PrimitiveTypeCode.UInt32
            or PrimitiveTypeCode.Int64  or PrimitiveTypeCode.UInt64
            or PrimitiveTypeCode.Single or PrimitiveTypeCode.Double
            or PrimitiveTypeCode.IntPtr or PrimitiveTypeCode.UIntPtr => "number",
        _ => "any",
    };

    public string GetTypeFromDefinition(MetadataReader r, TypeDefinitionHandle h, byte _)
    {
        var td = r.GetTypeDefinition(h);
        var fn = Helpers.FullName(r.GetString(td.Namespace), r.GetString(td.Name));
        if (EnumNames.Contains(Helpers.StripArity(fn))) return "number";
        return Helpers.MapBuiltinType(fn);
    }

    public string GetTypeFromReference(MetadataReader r, TypeReferenceHandle h, byte _)
    {
        var tr = r.GetTypeReference(h);
        var fn = Helpers.FullName(r.GetString(tr.Namespace), r.GetString(tr.Name));
        if (EnumNames.Contains(Helpers.StripArity(fn))) return "number";
        return Helpers.MapBuiltinType(fn);
    }

    public string GetTypeFromSpecification(MetadataReader r, object? ctx, TypeSpecificationHandle h, byte _)
        => r.GetTypeSpecification(h).DecodeSignature(this, ctx);

    public string GetSZArrayType(string elem)                           => $"{elem}[]";
    public string GetArrayType(string elem, ArrayShape _)               => $"{elem}[]";
    public string GetByReferenceType(string elem)                       => elem;
    public string GetPointerType(string _)                              => "any";
    public string GetPinnedType(string elem)                            => elem;
    public string GetModifiedType(string _, string unmodified, bool __) => unmodified;
    public string GetFunctionPointerType(MethodSignature<string> _)     => "Function";
    public string GetGenericMethodParameter(object? _, int index)       => $"T{index}";
    public string GetGenericTypeParameter(object? _, int index)         => $"T{index}";
    public string GetGenericInstantiation(string generic, ImmutableArray<string> typeArgs)
        => $"{generic}<{string.Join(", ", typeArgs)}>";
}
