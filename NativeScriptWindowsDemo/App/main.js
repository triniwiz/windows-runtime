import { createButton, text, withContentPadding } from "./core/ui.js";
import { createDashboardPage } from "./features/dashboard.js";
import { createGettingStartedPage } from "./features/getting-started.js";
import { createSetupPage } from "./features/setup.js";
import { createComponentsPage } from "./features/components.js";
import { createExamplesPage } from "./features/examples.js";

console.log("NativeScriptWindowsDemo: booting");

function createShell() {
    console.log("NativeScriptWindowsDemo: createShell start");
    try {
        const navigation = new Windows.UI.Xaml.Controls.NavigationView();
        navigation.PaneTitle = "NativeScript Studio";
        navigation.IsSettingsVisible = false;
        navigation.PaneDisplayMode = Windows.UI.Xaml.Controls.NavigationViewPaneDisplayMode.Left;
        navigation.IsBackButtonVisible = Windows.UI.Xaml.Controls.NavigationViewBackButtonVisible.Collapsed;
        navigation.IsPaneToggleButtonVisible = true;

        console.log("NativeScriptWindowsDemo: createShell done (NavigationView)");
        return { kind: "navigation", navigation };
    } catch (error) {
        console.log(
            "NativeScriptWindowsDemo: NavigationView unavailable, falling back to SplitView",
            error && error.message ? error.message : error,
        );
    }

    const split = new Windows.UI.Xaml.Controls.SplitView();
    split.DisplayMode = Windows.UI.Xaml.Controls.SplitViewDisplayMode.Inline;
    split.IsPaneOpen = true;
    split.OpenPaneLength = 260;

    const pane = new Windows.UI.Xaml.Controls.StackPanel();
    pane.Spacing = 6;
    pane.Children.Append(text("NativeScript Studio", 22));
    pane.Children.Append(text("Windows runtime demo tabs", 12));

    split.Pane = pane;

    console.log("NativeScriptWindowsDemo: createShell done (SplitView fallback)");
    return { kind: "split", split, pane };
}

function createNavigationItem(key, label, iconGlyph) {
    const item = new Windows.UI.Xaml.Controls.NavigationViewItem();
    item.Tag = key;
    item.Content = String(label);

    const icon = new Windows.UI.Xaml.Controls.FontIcon();
    icon.Glyph = iconGlyph;
    item.Icon = icon;

    return item;
}

function createScrollablePage(content) {
    const scroll = new Windows.UI.Xaml.Controls.ScrollViewer();
    scroll.VerticalScrollBarVisibility = Windows.UI.Xaml.Controls.ScrollBarVisibility.Auto;
    scroll.HorizontalScrollBarVisibility = Windows.UI.Xaml.Controls.ScrollBarVisibility.Disabled;
    scroll.Content = withContentPadding(content);
    return scroll;
}

function buildApp() {
    console.log("NativeScriptWindowsDemo: buildApp start");

    const shell = createShell();
    const navItemsByKey = {};

    const pages = {
        dashboard: createScrollablePage(createDashboardPage(showPage)),
        gettingStarted: createScrollablePage(createGettingStartedPage()),
        setup: createScrollablePage(createSetupPage()),
        components: createScrollablePage(createComponentsPage()),
        examples: createScrollablePage(createExamplesPage()),
    };

    function showPage(key) {
        const page = pages[key] || pages.dashboard;
        if (shell.kind === "navigation") {
            const host = shell.contentHost;
            if (host && typeof host === "object") {
                host.Child = page;
            } else {
                shell.navigation.Content = page;
            }
            return;
        }

        shell.split.Content = page;
        shell.split.IsPaneOpen = true;
    }

    if (shell.kind === "navigation") {
        const host = new Windows.UI.Xaml.Controls.Border();
        shell.contentHost = host;
        shell.navigation.Content = host;

        const dashboardItem = createNavigationItem("dashboard", "Overview", "\uE80F");
        const gettingStartedItem = createNavigationItem("gettingStarted", "Getting Started", "\uE8FD");
        const setupItem = createNavigationItem("setup", "Setup", "\uE713");
        const componentsItem = createNavigationItem("components", "Components", "\uE8B8");
        const examplesItem = createNavigationItem("examples", "Examples", "\uE9CE");

        navItemsByKey.dashboard = dashboardItem;
        navItemsByKey.gettingStarted = gettingStartedItem;
        navItemsByKey.setup = setupItem;
        navItemsByKey.components = componentsItem;
        navItemsByKey.examples = examplesItem;

        shell.navigation.MenuItems.Append(dashboardItem);
        shell.navigation.MenuItems.Append(gettingStartedItem);
        shell.navigation.MenuItems.Append(setupItem);
        shell.navigation.MenuItems.Append(componentsItem);
        shell.navigation.MenuItems.Append(examplesItem);

        shell.navigation.SelectedItem = dashboardItem;
        host.Child = pages.dashboard;

        const resolveNavigationKey = function () {
            const keys = Object.keys(navItemsByKey);

            for (let i = 0; i < keys.length; i += 1) {
                const key = keys[i];
                const item = navItemsByKey[key];
                if (!item) {
                    continue;
                }

                try {
                    if (item.IsSelected) {
                        return key;
                    }
                } catch (_error) {
                }
            }

            return null;
        };

        shell.navigation.SelectionChanged = function () {
            const key = resolveNavigationKey();
            if (key && pages[key]) {
                showPage(key);
            }
        };
    } else {
        shell.pane.Children.Append(createButton("Overview", function () { showPage("dashboard"); }));
        shell.pane.Children.Append(createButton("Getting Started", function () { showPage("gettingStarted"); }));
        shell.pane.Children.Append(createButton("Setup", function () { showPage("setup"); }));
        shell.pane.Children.Append(createButton("Components", function () { showPage("components"); }));
        shell.pane.Children.Append(createButton("Examples", function () { showPage("examples"); }));
    }

    showPage("dashboard");
    console.log("NativeScriptWindowsDemo: buildApp done");
    return shell.kind === "navigation" ? shell.navigation : shell.split;
}

try {
    const window = Windows.UI.Xaml.Window.Current;
    window.Content = buildApp();
    window.Activate();
    console.log("NativeScriptWindowsDemo: loaded");
} catch (error) {
    console.log("NativeScriptWindowsDemo: failed", error && error.message ? error.message : error);
    throw error;
}
