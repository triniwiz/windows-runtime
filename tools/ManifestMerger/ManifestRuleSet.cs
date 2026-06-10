using System;
using System.Linq;
using System.Xml.Linq;

namespace NativeScript.Windows.ManifestMerger
{
    internal sealed class ManifestIdentityKey
    {
        public ManifestIdentityKey(string value, string predicate)
        {
            Value = value;
            Predicate = predicate;
        }

        public string Value { get; }

        public string Predicate { get; }
    }

    internal static class ManifestRuleSet
    {
        public static ManifestIdentityKey? GetIdentityKey(XElement element)
        {
            var parentName = element.Parent?.Name.LocalName;

            if (parentName == "Applications" && HasLocalName(element, "Application"))
            {
                return AttributeKey(element, "Id");
            }

            if (parentName == "Capabilities")
            {
                return AttributeKey(element, "Name", includeElementName: true);
            }

            if (parentName == "Extensions" && HasLocalName(element, "Extension"))
            {
                return AttributeKey(element, "Category", includeElementName: true) ??
                    AttributeKey(element, "Name", includeElementName: true);
            }

            if (HasExtensionAncestor(element))
            {
                return AttributeKey(element, "Name", includeElementName: true) ??
                    AttributeKey(element, "Id", includeElementName: true) ??
                    AttributeKey(element, "Category", includeElementName: true);
            }

            if (HasLocalName(element, "VisualElements") ||
                HasLocalName(element, "DefaultTile") ||
                HasLocalName(element, "SplashScreen") ||
                HasLocalName(element, "LockScreen") ||
                HasLocalName(element, "InitialRotationPreference"))
            {
                return new ManifestIdentityKey(element.Name.LocalName, "local-name()='" + element.Name.LocalName + "'");
            }

            return null;
        }

        public static XElement? FindTargetMatch(XElement sourceElement, XElement targetParent, ManifestMergePlan plan)
        {
            var key = GetIdentityKey(sourceElement);
            if (key != null)
            {
                var matches = targetParent.Elements()
                    .Where(candidate => candidate.Name.LocalName == sourceElement.Name.LocalName && HasIdentityValue(candidate, key.Value))
                    .ToList();

                if (matches.Count > 1)
                {
                    plan.AddDiagnostic(
                        ManifestDiagnosticSeverity.Warning,
                        ManifestDiagnosticCodes.AmbiguousElementMatch,
                        ManifestPaths.GetPath(targetParent) + "/" + sourceElement.Name.LocalName + "[" + key.Predicate + "]",
                        "Multiple target elements matched the same manifest identity; the first match will be used.");
                }

                return matches.FirstOrDefault();
            }

            var localNameMatches = targetParent.Elements()
                .Where(candidate => candidate.Name.LocalName == sourceElement.Name.LocalName)
                .ToList();

            if (localNameMatches.Count > 1)
            {
                plan.AddDiagnostic(
                    ManifestDiagnosticSeverity.Warning,
                    ManifestDiagnosticCodes.AmbiguousElementMatch,
                    ManifestPaths.GetPath(targetParent) + "/" + sourceElement.Name.LocalName,
                    "Multiple target elements share this local name and no manifest identity rule applies; the first match will be used.");
            }

            return localNameMatches.FirstOrDefault();
        }

        public static XElement? FindApplicationForVisualElements(XElement sourceVisualElements, XDocument target, ManifestMergePlan plan)
        {
            var sourceApplication = sourceVisualElements.Ancestors().FirstOrDefault(element => HasLocalName(element, "Application"));
            var sourceApplicationId = sourceApplication?.Attribute("Id")?.Value;

            if (!string.IsNullOrWhiteSpace(sourceApplicationId))
            {
                var matches = target.Descendants()
                    .Where(element => HasLocalName(element, "Application") && (string?)element.Attribute("Id") == sourceApplicationId)
                    .ToList();

                if (matches.Count > 1)
                {
                    plan.AddDiagnostic(
                        ManifestDiagnosticSeverity.Warning,
                        ManifestDiagnosticCodes.AmbiguousElementMatch,
                        "/Package/Applications/Application[@Id='" + sourceApplicationId + "']",
                        "Multiple target applications use this Id; the first match will receive VisualElements.");
                }

                if (matches.Count > 0)
                {
                    return matches[0];
                }
            }

            return target.Descendants().FirstOrDefault(element => HasLocalName(element, "Application"));
        }

        public static bool IsExcludedTopLevelSection(XElement element)
        {
            return HasLocalName(element, "Dependencies") ||
                HasLocalName(element, "Applications") ||
                HasLocalName(element, "Capabilities");
        }

        public static bool IsMergeableContainer(XElement element)
        {
            return HasLocalName(element, "Extensions") ||
                HasLocalName(element, "Capabilities") ||
                HasLocalName(element, "Properties") ||
                HasLocalName(element, "Resources") ||
                HasLocalName(element, "Dependencies");
        }

        public static bool HasLocalName(XElement element, string localName)
        {
            return element.Name.LocalName == localName;
        }

        private static ManifestIdentityKey? AttributeKey(XElement element, string attributeName, bool includeElementName = false)
        {
            var attribute = element.Attribute(attributeName);
            if (attribute == null || string.IsNullOrWhiteSpace(attribute.Value))
            {
                return null;
            }

            var value = includeElementName
                ? element.Name.LocalName + "|" + attributeName + "|" + attribute.Value
                : attributeName + "|" + attribute.Value;

            return new ManifestIdentityKey(value, "@" + attributeName + "='" + attribute.Value + "'");
        }

        private static bool HasIdentityValue(XElement element, string value)
        {
            var key = GetIdentityKey(element);
            return key != null && key.Value == value;
        }

        private static bool HasExtensionAncestor(XElement element)
        {
            return element.Ancestors().Any(ancestor => HasLocalName(ancestor, "Extension"));
        }
    }
}
