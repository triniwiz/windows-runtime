import {
    createButton,
    createCard,
    safeGetChildrenSize,
    text,
} from "../core/ui.js";

export function createComponentsPage() {
    const root = new Windows.UI.Xaml.Controls.StackPanel();
    root.Spacing = 12;

    root.Children.Append(text("Components Lab", 34));

    const description = text(
        "This tab demonstrates real WinRT controls manipulated from JavaScript event handlers.",
        14,
    );
    description.TextWrapping = Windows.UI.Xaml.TextWrapping.Wrap;
    root.Children.Append(description);

    const toggle = new Windows.UI.Xaml.Controls.ToggleSwitch();
    toggle.Header = "Enable runtime polish";
    toggle.IsOn = true;

    const slider = new Windows.UI.Xaml.Controls.Slider();
    slider.Minimum = 0;
    slider.Maximum = 100;
    slider.Value = 40;
    slider.Width = 360;

    const progress = new Windows.UI.Xaml.Controls.ProgressBar();
    progress.Minimum = 0;
    progress.Maximum = 100;
    progress.Value = 40;
    progress.Width = 360;

    const status = text("State: polish enabled at 40%", 13);

    const applyButton = createButton("Apply Slider To Progress", function () {
        progress.Value = slider.Value;
        status.Text = "State: " + (toggle.IsOn ? "polish enabled" : "polish disabled") + " at " + Math.round(slider.Value) + "%";
    });

    root.Children.Append(toggle);
    root.Children.Append(slider);
    root.Children.Append(progress);
    root.Children.Append(applyButton);
    root.Children.Append(status);

    root.Children.Append(
        createCard(
            "Copyable Pattern",
            "Instantiate controls, keep references, and update state on click handlers.",
        ),
    );

    console.log("NativeScriptWindowsDemo: components children", safeGetChildrenSize(root));
    return root;
}
