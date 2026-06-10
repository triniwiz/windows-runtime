using System.Xml.Linq;
using NativeScript.Windows.ManifestMerger;

var tests = new (string Name, Action Run)[]
{
    ("merges visual element attributes and children", MergeVisualElements),
    ("merges visual elements into the application with the same Id", MergeVisualElementsByApplicationId),
    ("merges capabilities without nesting capabilities", MergeCapabilities),
    ("adds missing top-level extension children only once", MergeTopLevelSections),
    ("reports conflicting attribute overwrites", ReportsConflicts),
    ("diff reports changes without mutating target", DiffDoesNotMutate),
    ("file diff reports changes without mutating target file", DiffFilesDoesNotMutate),
    ("validation reports unsafe manifests", ValidationReportsUnsafeManifest),
    ("validation failure prevents file write", ValidationFailurePreventsFileWrite),
    ("reports missing source file without writing target", MissingSourceFile)
};

foreach (var test in tests)
{
    test.Run();
    Console.WriteLine("PASS " + test.Name);
}

static void MergeVisualElements()
{
    var source = XDocument.Parse("""
        <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10" xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10">
          <Applications>
            <Application>
              <uap:VisualElements DisplayName="Source App" Description="Source description">
                <uap:SplashScreen Image="Assets\SplashScreen.png" />
              </uap:VisualElements>
            </Application>
          </Applications>
        </Package>
        """);
    var target = XDocument.Parse("""
        <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10" xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10">
          <Applications>
            <Application Id="App">
              <uap:VisualElements DisplayName="Target App">
                <uap:DefaultTile Wide310x150Logo="Assets\Wide.png" />
              </uap:VisualElements>
            </Application>
          </Applications>
        </Package>
        """);

    new AppxManifestMerger().Merge(source, target);

    var visualElements = target.Descendants().Single(element => element.Name.LocalName == "VisualElements");
    AssertEqual("Source App", (string?)visualElements.Attribute("DisplayName"), "DisplayName");
    AssertEqual("Source description", (string?)visualElements.Attribute("Description"), "Description");
    AssertEqual(1, visualElements.Elements().Count(element => element.Name.LocalName == "DefaultTile"), "DefaultTile count");
    AssertEqual(1, visualElements.Elements().Count(element => element.Name.LocalName == "SplashScreen"), "SplashScreen count");
}

static void MergeCapabilities()
{
    var source = XDocument.Parse("""
        <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10" xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities">
          <Capabilities>
            <Capability Name="internetClient" />
            <rescap:Capability Name="runFullTrust" />
          </Capabilities>
        </Package>
        """);
    var target = XDocument.Parse("""
        <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10" xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities">
          <Capabilities>
            <Capability Name="internetClient" />
          </Capabilities>
        </Package>
        """);

    new AppxManifestMerger().Merge(source, target);

    var capabilities = target.Root!.Elements().Single(element => element.Name.LocalName == "Capabilities");
    AssertEqual(0, capabilities.Elements().Count(element => element.Name.LocalName == "Capabilities"), "nested Capabilities count");
    AssertEqual(2, capabilities.Elements().Count(), "capability count");
}

static void MergeVisualElementsByApplicationId()
{
    var source = XDocument.Parse("""
        <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10" xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10">
          <Applications>
            <Application Id="Second">
              <uap:VisualElements DisplayName="Second Source" />
            </Application>
          </Applications>
        </Package>
        """);
    var target = XDocument.Parse("""
        <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10" xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10">
          <Applications>
            <Application Id="First">
              <uap:VisualElements DisplayName="First Target" />
            </Application>
            <Application Id="Second">
              <uap:VisualElements DisplayName="Second Target" />
            </Application>
          </Applications>
        </Package>
        """);

    new AppxManifestMerger().Merge(source, target);

    var apps = target.Descendants().Where(element => element.Name.LocalName == "Application").ToList();
    AssertEqual("First Target", (string?)apps[0].Elements().Single(element => element.Name.LocalName == "VisualElements").Attribute("DisplayName"), "first app display name");
    AssertEqual("Second Source", (string?)apps[1].Elements().Single(element => element.Name.LocalName == "VisualElements").Attribute("DisplayName"), "second app display name");
}

static void MergeTopLevelSections()
{
    var source = XDocument.Parse("""
        <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10" xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10">
          <Extensions>
            <uap:Extension Category="windows.protocol" />
          </Extensions>
        </Package>
        """);
    var target = XDocument.Parse("""
        <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10" xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10">
          <Extensions>
            <uap:Extension Category="windows.fileTypeAssociation" />
          </Extensions>
        </Package>
        """);

    var merger = new AppxManifestMerger();
    merger.Merge(source, target);
    merger.Merge(source, target);

    var extensions = target.Root!.Elements().Single(element => element.Name.LocalName == "Extensions");
    AssertEqual(2, extensions.Elements().Count(), "extension count");
}

static void ReportsConflicts()
{
    var source = XDocument.Parse("""
        <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10" xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10">
          <Applications>
            <Application>
              <uap:VisualElements DisplayName="Source App" />
            </Application>
          </Applications>
        </Package>
        """);
    var target = XDocument.Parse("""
        <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10" xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10">
          <Applications>
            <Application Id="App">
              <uap:VisualElements DisplayName="Target App" />
            </Application>
          </Applications>
        </Package>
        """);

    var plan = new AppxManifestMerger().Merge(source, target);

    AssertEqual(true, plan.Diagnostics.Any(diagnostic => diagnostic.Code == ManifestDiagnosticCodes.AttributeOverwrite), "has overwrite diagnostic");
    AssertEqual(true, plan.Changes.Any(change => change.Kind == ManifestChangeKind.ReplaceAttribute), "has replace attribute change");
}

static void DiffDoesNotMutate()
{
    var source = XDocument.Parse("""
        <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10">
          <Capabilities>
            <Capability Name="internetClient" />
          </Capabilities>
        </Package>
        """);
    var target = XDocument.Parse("""
        <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10">
        </Package>
        """);

    var before = target.ToString(SaveOptions.DisableFormatting);
    var plan = new AppxManifestMerger().Diff(source, target);

    AssertEqual(true, plan.Changes.Any(change => change.Kind == ManifestChangeKind.AddElement), "has add element diff");
    AssertEqual(before, target.ToString(SaveOptions.DisableFormatting), "target unchanged");
}

static void DiffFilesDoesNotMutate()
{
    var tempDirectory = Path.Combine(Path.GetTempPath(), "ManifestMergerTests", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(tempDirectory);

    try
    {
        var sourcePath = Path.Combine(tempDirectory, "Source.appxmanifest");
        var targetPath = Path.Combine(tempDirectory, "Package.appxmanifest");
        File.WriteAllText(sourcePath, """
            <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10">
              <Capabilities>
                <Capability Name="internetClient" />
              </Capabilities>
            </Package>
            """);
        File.WriteAllText(targetPath, """
            <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10">
            </Package>
            """);

        var before = File.ReadAllText(targetPath);
        var plan = new AppxManifestMerger().DiffFiles(sourcePath, targetPath);

        AssertEqual(true, plan.Changes.Any(change => change.Kind == ManifestChangeKind.AddElement), "has add element file diff");
        AssertEqual(before, File.ReadAllText(targetPath), "target file unchanged");
    }
    finally
    {
        Directory.Delete(tempDirectory, recursive: true);
    }
}

static void ValidationReportsUnsafeManifest()
{
    var manifest = XDocument.Parse("""
        <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10">
          <Applications>
            <Application>
            </Application>
          </Applications>
          <Capabilities>
            <Capabilities />
          </Capabilities>
        </Package>
        """);

    var diagnostics = new AppxManifestValidator().Validate(manifest);

    AssertEqual(true, diagnostics.Any(diagnostic => diagnostic.Severity == ManifestDiagnosticSeverity.Error), "has validation errors");
    AssertEqual(true, diagnostics.Any(diagnostic => diagnostic.Message.Contains("VisualElements")), "mentions VisualElements");
}

static void ValidationFailurePreventsFileWrite()
{
    var tempDirectory = Path.Combine(Path.GetTempPath(), "ManifestMergerTests", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(tempDirectory);

    try
    {
        var sourcePath = Path.Combine(tempDirectory, "Source.appxmanifest");
        var targetPath = Path.Combine(tempDirectory, "Package.appxmanifest");
        File.WriteAllText(sourcePath, """
            <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10">
              <Capabilities>
                <Capabilities />
              </Capabilities>
            </Package>
            """);
        File.WriteAllText(targetPath, """
            <Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10">
              <Applications>
                <Application Id="App">
                  <VisualElements DisplayName="Target" />
                </Application>
              </Applications>
            </Package>
            """);

        var before = File.ReadAllText(targetPath);
        var result = new AppxManifestMerger().MergeFiles(sourcePath, targetPath, new ManifestMergeOptions { ValidateMergedManifest = true });

        AssertEqual(true, result.Plan.HasErrors, "has validation errors");
        AssertEqual(before, File.ReadAllText(targetPath), "target file unchanged");
    }
    finally
    {
        Directory.Delete(tempDirectory, recursive: true);
    }
}

static void MissingSourceFile()
{
    var tempDirectory = Path.Combine(Path.GetTempPath(), "ManifestMergerTests", Guid.NewGuid().ToString("N"));
    Directory.CreateDirectory(tempDirectory);

    try
    {
        var targetPath = Path.Combine(tempDirectory, "Package.appxmanifest");
        File.WriteAllText(targetPath, "<Package />");

        var result = new AppxManifestMerger().MergeFiles(
            Path.Combine(tempDirectory, "Missing.appxmanifest"),
            targetPath);

        AssertEqual(ManifestMergeStatus.SourceMissing, result.Status, "merge status");
        AssertEqual("<Package />", File.ReadAllText(targetPath), "target contents");
    }
    finally
    {
        Directory.Delete(tempDirectory, recursive: true);
    }
}

static void AssertEqual<T>(T expected, T actual, string name)
{
    if (!EqualityComparer<T>.Default.Equals(expected, actual))
    {
        throw new InvalidOperationException(name + ": expected '" + expected + "' but got '" + actual + "'.");
    }
}
