using System;
using System.IO;
using System.Linq;
using System.Xml.Linq;

namespace NativeScript.Windows.ManifestMerger
{
    public sealed class AppxManifestMerger
    {
        public ManifestMergeResult MergeFiles(string sourceManifest, string targetManifest, ManifestMergeOptions? options = null)
        {
            if (string.IsNullOrWhiteSpace(sourceManifest))
            {
                throw new ArgumentException("Source manifest path is required.", nameof(sourceManifest));
            }

            if (string.IsNullOrWhiteSpace(targetManifest))
            {
                throw new ArgumentException("Target manifest path is required.", nameof(targetManifest));
            }

            if (!File.Exists(sourceManifest))
            {
                return new ManifestMergeResult(
                    ManifestMergeStatus.SourceMissing,
                    "No source manifest found: " + sourceManifest,
                    new ManifestMergePlan());
            }

            if (!File.Exists(targetManifest))
            {
                return new ManifestMergeResult(
                    ManifestMergeStatus.TargetMissing,
                    "No target manifest found: " + targetManifest,
                    new ManifestMergePlan());
            }

            var sourceFullPath = Path.GetFullPath(sourceManifest);
            var targetFullPath = Path.GetFullPath(targetManifest);
            if (string.Equals(sourceFullPath, targetFullPath, StringComparison.OrdinalIgnoreCase))
            {
                return new ManifestMergeResult(
                    ManifestMergeStatus.SameFile,
                    "Source and target manifest are the same file: " + targetManifest,
                    new ManifestMergePlan());
            }

            var source = XDocument.Load(sourceManifest);
            var target = XDocument.Load(targetManifest);
            var plan = Merge(source, target, options);
            if (plan.HasErrors)
            {
                return new ManifestMergeResult(
                    ManifestMergeStatus.ValidationFailed,
                    "Merged manifest from " + sourceManifest + " into " + targetManifest + " was not saved because validation failed.",
                    plan);
            }

            target.Save(targetManifest);

            return new ManifestMergeResult(
                ManifestMergeStatus.Merged,
                "Merged manifest from " + sourceManifest + " into " + targetManifest,
                plan);
        }

        public ManifestMergePlan Merge(XDocument source, XDocument target, ManifestMergeOptions? options = null)
        {
            ValidateDocuments(source, target);

            var plan = new ManifestMergePlan();
            MergeCore(source, target, plan, dryRun: false);
            ValidateIfRequested(target, options, plan);
            return plan;
        }

        public ManifestMergePlan PlanMerge(XDocument source, XDocument target, ManifestMergeOptions? options = null)
        {
            ValidateDocuments(source, target);

            var targetClone = new XDocument(target);
            var plan = new ManifestMergePlan();
            MergeCore(source, targetClone, plan, dryRun: false);
            ValidateIfRequested(targetClone, options, plan);
            return plan;
        }

        public ManifestMergePlan Diff(XDocument source, XDocument target, ManifestMergeOptions? options = null)
        {
            return PlanMerge(source, target, options);
        }

        public ManifestMergePlan DiffFiles(string sourceManifest, string targetManifest, ManifestMergeOptions? options = null)
        {
            if (string.IsNullOrWhiteSpace(sourceManifest))
            {
                throw new ArgumentException("Source manifest path is required.", nameof(sourceManifest));
            }

            if (string.IsNullOrWhiteSpace(targetManifest))
            {
                throw new ArgumentException("Target manifest path is required.", nameof(targetManifest));
            }

            return Diff(XDocument.Load(sourceManifest), XDocument.Load(targetManifest), options);
        }

        private static void MergeCore(XDocument source, XDocument target, ManifestMergePlan plan, bool dryRun)
        {
            MergeVisualElements(source, target, plan, dryRun);
            MergeCapabilities(source, target, plan, dryRun);
            MergeTopLevelSections(source, target, plan, dryRun);
        }

        private static void MergeVisualElements(XDocument source, XDocument target, ManifestMergePlan plan, bool dryRun)
        {
            foreach (var sourceVisualElements in source.Descendants().Where(element => ManifestRuleSet.HasLocalName(element, "VisualElements")))
            {
                var targetApplication = ManifestRuleSet.FindApplicationForVisualElements(sourceVisualElements, target, plan);
                if (targetApplication == null)
                {
                    AddElement(target.Root!, sourceVisualElements, plan, dryRun);
                    continue;
                }

                MergeElementIntoParent(sourceVisualElements, targetApplication, plan, dryRun);
            }
        }

        private static void MergeCapabilities(XDocument source, XDocument target, ManifestMergePlan plan, bool dryRun)
        {
            foreach (var sourceCapabilities in source.Descendants().Where(element => ManifestRuleSet.HasLocalName(element, "Capabilities")))
            {
                var targetCapabilities = target.Root!.Elements()
                    .FirstOrDefault(element => ManifestRuleSet.HasLocalName(element, "Capabilities"));

                if (targetCapabilities == null)
                {
                    AddElement(target.Root, sourceCapabilities, plan, dryRun);
                    continue;
                }

                MergeCollectionChildren(sourceCapabilities, targetCapabilities, plan, dryRun);
            }
        }

        private static void MergeTopLevelSections(XDocument source, XDocument target, ManifestMergePlan plan, bool dryRun)
        {
            foreach (var sourceTopLevel in source.Root!.Elements())
            {
                if (ManifestRuleSet.IsExcludedTopLevelSection(sourceTopLevel))
                {
                    continue;
                }

                var targetTopLevel = ManifestRuleSet.FindTargetMatch(sourceTopLevel, target.Root!, plan);
                if (targetTopLevel == null)
                {
                    AddElement(target.Root!, sourceTopLevel, plan, dryRun);
                    continue;
                }

                MergeElement(sourceTopLevel, targetTopLevel, plan, dryRun);
            }
        }

        private static void MergeElementIntoParent(XElement sourceElement, XElement targetParent, ManifestMergePlan plan, bool dryRun)
        {
            var targetElement = ManifestRuleSet.FindTargetMatch(sourceElement, targetParent, plan);
            if (targetElement == null)
            {
                AddElement(targetParent, sourceElement, plan, dryRun);
                return;
            }

            MergeElement(sourceElement, targetElement, plan, dryRun);
        }

        private static void MergeElement(XElement sourceElement, XElement targetElement, ManifestMergePlan plan, bool dryRun)
        {
            if (XNode.DeepEquals(sourceElement, targetElement))
            {
                plan.AddChange(
                    ManifestChangeKind.SkipEquivalent,
                    ManifestPaths.GetPath(targetElement),
                    "Skipped equivalent element.");
                return;
            }

            plan.AddChange(
                ManifestChangeKind.MergeElement,
                ManifestPaths.GetPath(targetElement),
                "Merged source element into matching target element.");

            plan.AddDiagnostic(
                ManifestDiagnosticSeverity.Warning,
                ManifestDiagnosticCodes.ElementCollision,
                ManifestPaths.GetPath(targetElement),
                "Source and target elements share a manifest identity but are not equivalent.");

            foreach (var attribute in sourceElement.Attributes())
            {
                MergeAttribute(targetElement, attribute, plan, dryRun);
            }

            foreach (var sourceChild in sourceElement.Elements())
            {
                MergeElementIntoParent(sourceChild, targetElement, plan, dryRun);
            }
        }

        private static void MergeCollectionChildren(XElement sourceSection, XElement targetSection, ManifestMergePlan plan, bool dryRun)
        {
            foreach (var sourceChild in sourceSection.Elements())
            {
                MergeElementIntoParent(sourceChild, targetSection, plan, dryRun);
            }
        }

        private static void MergeAttribute(XElement targetElement, XAttribute sourceAttribute, ManifestMergePlan plan, bool dryRun)
        {
            var targetAttribute = targetElement.Attribute(sourceAttribute.Name);
            var path = ManifestPaths.GetAttributePath(targetElement, sourceAttribute);

            if (targetAttribute == null)
            {
                plan.AddChange(ManifestChangeKind.AddAttribute, path, "Added source attribute.");
                if (!dryRun)
                {
                    targetElement.SetAttributeValue(sourceAttribute.Name, sourceAttribute.Value);
                }

                return;
            }

            if (targetAttribute.Value == sourceAttribute.Value)
            {
                return;
            }

            plan.AddChange(ManifestChangeKind.ReplaceAttribute, path, "Replaced target attribute value.");
            plan.AddDiagnostic(
                ManifestDiagnosticSeverity.Warning,
                ManifestDiagnosticCodes.AttributeOverwrite,
                path,
                "Attribute value changed from '" + targetAttribute.Value + "' to '" + sourceAttribute.Value + "'.");

            if (!dryRun)
            {
                targetAttribute.Value = sourceAttribute.Value;
            }
        }

        private static void AddElement(XElement targetParent, XElement sourceElement, ManifestMergePlan plan, bool dryRun)
        {
            var targetPath = ManifestPaths.GetPath(targetParent) + "/" + sourceElement.Name.LocalName;
            plan.AddChange(
                ManifestChangeKind.AddElement,
                targetPath,
                "Added source element.");

            if (!dryRun)
            {
                targetParent.Add(new XElement(sourceElement));
            }
        }

        private static void ValidateIfRequested(XDocument target, ManifestMergeOptions? options, ManifestMergePlan plan)
        {
            if (options?.ValidateMergedManifest != true)
            {
                return;
            }

            foreach (var diagnostic in new AppxManifestValidator().Validate(target, options))
            {
                plan.AddDiagnostic(diagnostic.Severity, diagnostic.Code, diagnostic.Path, diagnostic.Message);
            }
        }

        private static void ValidateDocuments(XDocument source, XDocument target)
        {
            if (source == null)
            {
                throw new ArgumentNullException(nameof(source));
            }

            if (target == null)
            {
                throw new ArgumentNullException(nameof(target));
            }

            if (source.Root == null)
            {
                throw new InvalidOperationException("Source manifest has no root element.");
            }

            if (target.Root == null)
            {
                throw new InvalidOperationException("Target manifest has no root element.");
            }
        }
    }
}
