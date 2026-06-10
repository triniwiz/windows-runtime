using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Xml;
using System.Xml.Linq;
using System.Xml.Schema;

namespace NativeScript.Windows.ManifestMerger
{
    public sealed class AppxManifestValidator
    {
        public IReadOnlyList<ManifestMergeDiagnostic> ValidateFile(string manifestPath, ManifestMergeOptions? options = null)
        {
            if (string.IsNullOrWhiteSpace(manifestPath))
            {
                throw new ArgumentException("Manifest path is required.", nameof(manifestPath));
            }

            return Validate(XDocument.Load(manifestPath), options);
        }

        public IReadOnlyList<ManifestMergeDiagnostic> Validate(XDocument manifest, ManifestMergeOptions? options = null)
        {
            if (manifest == null)
            {
                throw new ArgumentNullException(nameof(manifest));
            }

            options = options ?? new ManifestMergeOptions();
            var diagnostics = new List<ManifestMergeDiagnostic>();
            ValidateStructure(manifest, diagnostics, options);

            if (options.SchemaFiles.Count > 0)
            {
                ValidateSchemas(manifest, diagnostics, options);
            }

            return diagnostics;
        }

        private static void ValidateStructure(XDocument manifest, IList<ManifestMergeDiagnostic> diagnostics, ManifestMergeOptions options)
        {
            var severity = options.TreatValidationErrorsAsWarnings
                ? ManifestDiagnosticSeverity.Warning
                : ManifestDiagnosticSeverity.Error;

            if (manifest.Root == null)
            {
                Add(diagnostics, severity, "/", "Manifest has no root element.");
                return;
            }

            if (manifest.Root.Name.LocalName != "Package")
            {
                Add(diagnostics, severity, ManifestPaths.GetPath(manifest.Root), "MSIX manifests must use Package as the root element.");
            }

            var applications = manifest.Root.Elements().Where(element => ManifestRuleSet.HasLocalName(element, "Applications")).ToList();
            if (applications.Count == 0)
            {
                Add(diagnostics, severity, ManifestPaths.GetPath(manifest.Root), "Manifest is missing an Applications element.");
            }

            if (applications.Count > 1)
            {
                Add(diagnostics, severity, ManifestPaths.GetPath(manifest.Root) + "/Applications", "Manifest contains multiple Applications elements.");
            }

            foreach (var application in applications.SelectMany(section => section.Elements().Where(element => ManifestRuleSet.HasLocalName(element, "Application"))))
            {
                if (application.Attribute("Id") == null)
                {
                    Add(diagnostics, severity, ManifestPaths.GetPath(application), "Application is missing required Id attribute.");
                }

                var visualElements = application.Elements().Where(element => ManifestRuleSet.HasLocalName(element, "VisualElements")).ToList();
                if (visualElements.Count == 0)
                {
                    Add(diagnostics, severity, ManifestPaths.GetPath(application), "Application is missing VisualElements.");
                }

                if (visualElements.Count > 1)
                {
                    Add(diagnostics, severity, ManifestPaths.GetPath(application) + "/VisualElements", "Application contains multiple VisualElements elements.");
                }
            }

            foreach (var capabilities in manifest.Descendants().Where(element => ManifestRuleSet.HasLocalName(element, "Capabilities")))
            {
                if (capabilities.Parent != manifest.Root)
                {
                    Add(diagnostics, severity, ManifestPaths.GetPath(capabilities), "Capabilities must be a direct Package child.");
                }

                foreach (var child in capabilities.Elements())
                {
                    if (ManifestRuleSet.HasLocalName(child, "Capabilities"))
                    {
                        Add(diagnostics, severity, ManifestPaths.GetPath(child), "Capabilities cannot be nested inside Capabilities.");
                    }

                    if (child.Attribute("Name") == null)
                    {
                        Add(diagnostics, severity, ManifestPaths.GetPath(child), "Capability elements should declare a Name attribute.");
                    }
                }
            }
        }

        private static void ValidateSchemas(XDocument manifest, IList<ManifestMergeDiagnostic> diagnostics, ManifestMergeOptions options)
        {
            var schemas = new XmlSchemaSet();
            foreach (var schemaFile in options.SchemaFiles)
            {
                if (!File.Exists(schemaFile))
                {
                    Add(diagnostics, ManifestDiagnosticSeverity.Error, schemaFile, "Schema file was not found.");
                    continue;
                }

                schemas.Add(null, schemaFile);
            }

            manifest.Validate(schemas, (sender, args) =>
            {
                var severity = options.TreatValidationErrorsAsWarnings || args.Severity == XmlSeverityType.Warning
                    ? ManifestDiagnosticSeverity.Warning
                    : ManifestDiagnosticSeverity.Error;

                Add(diagnostics, severity, "/", args.Message);
            });
        }

        private static void Add(IList<ManifestMergeDiagnostic> diagnostics, ManifestDiagnosticSeverity severity, string path, string message)
        {
            diagnostics.Add(new ManifestMergeDiagnostic(
                severity,
                ManifestDiagnosticCodes.Validation,
                path,
                message));
        }
    }
}
