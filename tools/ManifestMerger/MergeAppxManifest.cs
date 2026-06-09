using Microsoft.Build.Framework;
using Microsoft.Build.Utilities;
using NativeScript.Windows.ManifestMerger;

public sealed class MergeAppxManifest : Task
{
    [Required]
    public string SourceManifest { get; set; } = string.Empty;

    [Required]
    public string TargetManifest { get; set; } = string.Empty;

    public bool ValidateMergedManifest { get; set; }

    public bool TreatValidationErrorsAsWarnings { get; set; }

    public bool LogMergeDiagnosticsAsWarnings { get; set; }

    public ITaskItem[] SchemaFiles { get; set; } = new ITaskItem[0];

    public override bool Execute()
    {
        try
        {
            var options = new ManifestMergeOptions
            {
                ValidateMergedManifest = ValidateMergedManifest,
                TreatValidationErrorsAsWarnings = TreatValidationErrorsAsWarnings
            };

            foreach (var schemaFile in SchemaFiles)
            {
                options.SchemaFiles.Add(schemaFile.ItemSpec);
            }

            var result = new AppxManifestMerger().MergeFiles(SourceManifest, TargetManifest, options);
            var importance = result.WroteTarget ? MessageImportance.High : MessageImportance.Low;
            Log.LogMessage(importance, result.Message);

            foreach (var change in result.Changes)
            {
                Log.LogMessage(MessageImportance.Low, "{0}: {1} ({2})", change.Kind, change.Path, change.Message);
            }

            foreach (var diagnostic in result.Diagnostics)
            {
                if (diagnostic.Severity == ManifestDiagnosticSeverity.Error)
                {
                    Log.LogError(null, diagnostic.Code, null, SourceManifest, 0, 0, 0, 0, "{0}: {1}", diagnostic.Path, diagnostic.Message);
                    continue;
                }

                if (diagnostic.Severity == ManifestDiagnosticSeverity.Warning)
                {
                    if (diagnostic.Code == ManifestDiagnosticCodes.Validation || LogMergeDiagnosticsAsWarnings)
                    {
                        Log.LogWarning(null, diagnostic.Code, null, SourceManifest, 0, 0, 0, 0, "{0}: {1}", diagnostic.Path, diagnostic.Message);
                    }
                    else
                    {
                        Log.LogMessage(MessageImportance.High, "{0} {1}: {2}", diagnostic.Code, diagnostic.Path, diagnostic.Message);
                    }

                    continue;
                }

                Log.LogMessage(MessageImportance.Low, "{0} {1}: {2}", diagnostic.Code, diagnostic.Path, diagnostic.Message);
            }

            return !Log.HasLoggedErrors && !result.Plan.HasErrors;
        }
        catch (System.Exception ex)
        {
            Log.LogErrorFromException(ex, true);
            return false;
        }
    }
}
