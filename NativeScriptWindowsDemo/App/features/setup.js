import {
    createButton,
    createCard,
    openExternalUrl,
    safeGetChildrenSize,
    text,
} from "../core/ui.js";

export function createSetupPage() {
    const root = new Windows.UI.Xaml.Controls.StackPanel();
    root.Spacing = 12;

    root.Children.Append(text("Setup", 34));

    const body = text(
        "Bootstrap your own app from the NativeScript template and map these demo tabs to your production flows.",
        14,
    );
    body.TextWrapping = Windows.UI.Xaml.TextWrapping.Wrap;
    root.Children.Append(body);

    root.Children.Append(
        createButton("Open NativeScript Starter Templates", function () {
            openExternalUrl("https://github.com/NativeScript");
        }),
    );

    root.Children.Append(
        createCard(
            "Runtime + UI",
            "This sample runs pure JavaScript while driving native WinRT controls with no JSX/TSX layer.",
        ),
    );
    root.Children.Append(
        createCard(
            "Modular Features",
            "Each tab lives in its own file so teams can evolve pages independently.",
        ),
    );

    console.log("NativeScriptWindowsDemo: setup children", safeGetChildrenSize(root));
    return root;
}
