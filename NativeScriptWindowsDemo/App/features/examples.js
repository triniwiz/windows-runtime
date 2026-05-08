import {
    createButton,
    createCard,
    safeGetChildrenSize,
    text,
} from "../core/ui.js";

export function createExamplesPage() {
    const root = new Windows.UI.Xaml.Controls.StackPanel();
    root.Spacing = 12;

    root.Children.Append(text("Examples", 34));

    const intro = text("Interactive examples implemented in plain JavaScript.", 14);
    intro.TextWrapping = Windows.UI.Xaml.TextWrapping.Wrap;
    root.Children.Append(intro);

    const counterTitle = text("Counter Demo", 20);
    root.Children.Append(counterTitle);

    const counterRow = new Windows.UI.Xaml.Controls.StackPanel();
    counterRow.Orientation = Windows.UI.Xaml.Controls.Orientation.Horizontal;
    counterRow.Spacing = 10;

    const valueText = text("0", 24);
    let value = 0;

    counterRow.Children.Append(
        createButton("-", function () {
            value = Math.max(value - 1, 0);
            valueText.Text = String(value);
        }),
    );
    counterRow.Children.Append(valueText);
    counterRow.Children.Append(
        createButton("+", function () {
            value += 1;
            valueText.Text = String(value);
        }),
    );

    root.Children.Append(counterRow);

    root.Children.Append(text("Action Feed", 20));

    const feed = new Windows.UI.Xaml.Controls.StackPanel();
    feed.Spacing = 4;

    const addFeedEntry = function (entry) {
        if (feed.Children.Size > 6) {
            feed.Children.RemoveAt(0);
        }
        feed.Children.Append(text("- " + entry, 13));
    };

    const examplesRow = new Windows.UI.Xaml.Controls.StackPanel();
    examplesRow.Orientation = Windows.UI.Xaml.Controls.Orientation.Horizontal;
    examplesRow.Spacing = 8;

    examplesRow.Children.Append(
        createButton("Track Counter", function () {
            addFeedEntry("Counter currently at " + value);
        }),
    );

    examplesRow.Children.Append(
        createButton("Simulate Save", function () {
            addFeedEntry("Persisted runtime demo state");
        }),
    );

    root.Children.Append(examplesRow);
    root.Children.Append(feed);

    root.Children.Append(
        createCard(
            "Why this matters",
            "You can compose useful behavior with plain JS functions and native controls, then scale into larger modules.",
        ),
    );

    console.log("NativeScriptWindowsDemo: examples children", safeGetChildrenSize(root));
    return root;
}
