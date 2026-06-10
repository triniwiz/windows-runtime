using System;
using System.Collections.Generic;
using System.Linq;

namespace NativeScript.Windows.ManifestMerger
{
    public enum ManifestMergeStatus
    {
        Merged,
        SourceMissing,
        TargetMissing,
        SameFile,
        ValidationFailed
    }

    public enum ManifestChangeKind
    {
        AddElement,
        AddAttribute,
        ReplaceAttribute,
        MergeElement,
        SkipEquivalent
    }

    public enum ManifestDiagnosticSeverity
    {
        Info,
        Warning,
        Error
    }

    public static class ManifestDiagnosticCodes
    {
        public const string AttributeOverwrite = "MM001";
        public const string ElementCollision = "MM002";
        public const string AmbiguousElementMatch = "MM003";
        public const string Validation = "MM004";
    }

    public sealed class ManifestMergeOptions
    {
        public bool ValidateMergedManifest { get; set; }

        public bool TreatValidationErrorsAsWarnings { get; set; }

        public IList<string> SchemaFiles { get; } = new List<string>();
    }

    public sealed class ManifestMergeChange
    {
        public ManifestMergeChange(ManifestChangeKind kind, string path, string message)
        {
            Kind = kind;
            Path = path;
            Message = message;
        }

        public ManifestChangeKind Kind { get; }

        public string Path { get; }

        public string Message { get; }
    }

    public sealed class ManifestMergeDiagnostic
    {
        public ManifestMergeDiagnostic(ManifestDiagnosticSeverity severity, string code, string path, string message)
        {
            Severity = severity;
            Code = code;
            Path = path;
            Message = message;
        }

        public ManifestDiagnosticSeverity Severity { get; }

        public string Code { get; }

        public string Path { get; }

        public string Message { get; }
    }

    public sealed class ManifestMergePlan
    {
        private readonly List<ManifestMergeChange> changes = new List<ManifestMergeChange>();
        private readonly List<ManifestMergeDiagnostic> diagnostics = new List<ManifestMergeDiagnostic>();

        public IReadOnlyList<ManifestMergeChange> Changes => changes;

        public IReadOnlyList<ManifestMergeDiagnostic> Diagnostics => diagnostics;

        public bool HasChanges => changes.Any(change => change.Kind != ManifestChangeKind.SkipEquivalent);

        public bool HasErrors => diagnostics.Any(diagnostic => diagnostic.Severity == ManifestDiagnosticSeverity.Error);

        internal void AddChange(ManifestChangeKind kind, string path, string message)
        {
            changes.Add(new ManifestMergeChange(kind, path, message));
        }

        internal void AddDiagnostic(ManifestDiagnosticSeverity severity, string code, string path, string message)
        {
            diagnostics.Add(new ManifestMergeDiagnostic(severity, code, path, message));
        }
    }

    public sealed class ManifestMergeResult
    {
        public ManifestMergeResult(ManifestMergeStatus status, string message, ManifestMergePlan plan)
        {
            Status = status;
            Message = message;
            Plan = plan;
        }

        public ManifestMergeStatus Status { get; }

        public string Message { get; }

        public ManifestMergePlan Plan { get; }

        public IReadOnlyList<ManifestMergeDiagnostic> Diagnostics => Plan.Diagnostics;

        public IReadOnlyList<ManifestMergeChange> Changes => Plan.Changes;

        public bool WroteTarget => Status == ManifestMergeStatus.Merged;
    }
}
