import {
    createBrandImage,
    createButton,
    createCard,
    openExternalUrl,
    safeGetChildrenSize,
    text,
} from "../core/ui.js";

export function createDashboardPage(onNavigate) {
    const root = new Windows.UI.Xaml.Controls.StackPanel();
    root.Spacing = 14;

    root.Children.Append(text("NativeScript for Windows", 42));

    const subtitle = text(
        "Build native desktop apps with JavaScript, first-class WinRT interop, and a runtime that feels fast and direct.",
        16,
    );
    subtitle.TextWrapping = Windows.UI.Xaml.TextWrapping.Wrap;
    root.Children.Append(subtitle);

    root.Children.Append(createBrandImage(380, 200));

    root.Children.Append(
        createCard(
            "Single JavaScript Runtime",
            "Use one language across startup, UI composition, interop calls, and event binding.",
        ),
    );
    root.Children.Append(
        createCard(
            "Real Native Controls",
            "NavigationView, ToggleSwitch, SplitView, and every WinRT type are available directly from JavaScript.",
        ),
    );
    root.Children.Append(
        createCard(
            "Production-minded Tooling",
            "Build from Rust crates, test with sample apps, and iterate quickly without leaving native APIs behind.",
        ),
    );

    const ctaRow = new Windows.UI.Xaml.Controls.StackPanel();
    ctaRow.Orientation = Windows.UI.Xaml.Controls.Orientation.Horizontal;
    ctaRow.Spacing = 8;

    ctaRow.Children.Append(
        createButton("Explore Components Demo", function () {
            if (typeof onNavigate === "function") {
                onNavigate("components");
            }
        }),
    );

    ctaRow.Children.Append(
        createButton("Visit NativeScript.org", function () {
            openExternalUrl("https://nativescript.org");
        }),
    );

    root.Children.Append(ctaRow);

    root.Children.Append(
        text("Copy the patterns from each tab to build your own native app shell.", 13),
    );

    console.log("NativeScriptWindowsDemo: dashboard children", safeGetChildrenSize(root));
    return root;
}
