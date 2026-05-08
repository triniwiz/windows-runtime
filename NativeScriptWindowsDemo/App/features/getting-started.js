import {
    createBrandImage,
    createCard,
    safeGetChildrenSize,
    text,
} from "../core/ui.js";

export function createGettingStartedPage() {
    const root = new Windows.UI.Xaml.Controls.StackPanel();
    root.Spacing = 12;

    root.Children.Append(text("Getting Started", 34));
    root.Children.Append(createBrandImage(220, 120));

    const intro = text("Hello NativeScript Windows", 22);
    root.Children.Append(intro);

    const sub = text("Let us build something native together.", 16);
    sub.TextWrapping = Windows.UI.Xaml.TextWrapping.Wrap;
    root.Children.Append(sub);

    root.Children.Append(
        createCard(
            "1. Explore Native Controls",
            "Use the Components tab to try ToggleSwitch, Slider, ProgressRing, and native event wiring.",
        ),
    );
    root.Children.Append(
        createCard(
            "2. Explore Full Examples",
            "Use the Examples tab for interactive counter and list patterns in plain JavaScript.",
        ),
    );
    root.Children.Append(
        createCard(
            "3. Ship and Share",
            "Use this shell as your launchpad for real desktop experiences backed by Rust runtime speed.",
        ),
    );

    console.log("NativeScriptWindowsDemo: getting-started children", safeGetChildrenSize(root));
    return root;
}
