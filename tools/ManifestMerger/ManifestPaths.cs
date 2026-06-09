using System;
using System.Collections.Generic;
using System.Linq;
using System.Xml.Linq;

namespace NativeScript.Windows.ManifestMerger
{
    internal static class ManifestPaths
    {
        public static string GetPath(XElement element)
        {
            var parts = new Stack<string>();
            var current = element;
            while (current != null)
            {
                parts.Push(FormatElement(current));
                current = current.Parent;
            }

            return "/" + string.Join("/", parts.ToArray());
        }

        public static string GetAttributePath(XElement element, XAttribute attribute)
        {
            return GetPath(element) + "/@" + attribute.Name.LocalName;
        }

        private static string FormatElement(XElement element)
        {
            var key = ManifestRuleSet.GetIdentityKey(element);
            if (key == null)
            {
                return element.Name.LocalName;
            }

            return element.Name.LocalName + "[" + key.Predicate + "]";
        }
    }
}
