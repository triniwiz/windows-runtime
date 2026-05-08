console.log("NativeScriptWindowsDemo: booting");

const delegateKeepAlive = [];

function text(value, size) {
    const control = new Windows.UI.Xaml.Controls.TextBlock();
    control.Text = String(value);
    if (size) {
        control.FontSize = size;
    }
    return control;
}

function resolveType(typeName) {
    const parts = String(typeName).split(".");
    let current = globalThis;
    for (let i = 0; i < parts.length; i++) {
        current = current && current[parts[i]];
    }
    return current;
}

function createTypedDelegate(typeName, callback) {
    const TypeCtor = resolveType(typeName);
    if (typeof TypeCtor !== "function") {
        return null;
    }

    try {
        const delegate = new TypeCtor({
            Invoke: function () {
                return callback.apply(null, arguments);
            },
        });
        delegateKeepAlive.push(delegate);
        return delegate;
    } catch (_error) {
        return null;
    }
}

function tryCreateAcrylicBrush() {
    const AcrylicBrush = resolveType("Windows.UI.Xaml.Media.AcrylicBrush");
    if (typeof AcrylicBrush !== "function") {
        return null;
    }

    try {
        const brush = new AcrylicBrush();
        brush.TintOpacity = 0.22;
        return brush;
    } catch (_error) {
        return null;
    }
}

function bindClick(button, onClick) {
    const callback = typeof onClick === "function" ? onClick : function () {};
    const delegate = createTypedDelegate("Windows.UI.Xaml.RoutedEventHandler", callback);

    if (delegate && typeof button.add_Click === "function") {
        button.add_Click(delegate);
        return;
    }

    button.Click = delegate || callback;
}

function createButton(label, onClick) {
    const button = new Windows.UI.Xaml.Controls.Button();
    button.Height = 42;
    button.Content = String(label);
    bindClick(button, onClick);
    return button;
}

function createCard(titleValue, bodyValue) {
    const border = new Windows.UI.Xaml.Controls.Border();

    const stack = new Windows.UI.Xaml.Controls.StackPanel();
    stack.Spacing = 8;

    const titleText = text(titleValue, 19);
    const bodyText = text(bodyValue, 14);
    bodyText.TextWrapping = Windows.UI.Xaml.TextWrapping.Wrap;

    const tagLine = text("runtime-ready", 12);

    stack.Children.Append(titleText);
    stack.Children.Append(bodyText);
    stack.Children.Append(tagLine);

    border.Child = stack;
    return border;
}

function withContentPadding(content) {
    const outer = new Windows.UI.Xaml.Controls.StackPanel();

    const topSpacer = new Windows.UI.Xaml.Controls.Border();
    topSpacer.Height = 18;
    outer.Children.Append(topSpacer);

    const row = new Windows.UI.Xaml.Controls.StackPanel();
    row.Orientation = Windows.UI.Xaml.Controls.Orientation.Horizontal;

    const leftSpacer = new Windows.UI.Xaml.Controls.Border();
    leftSpacer.Width = 24;

    row.Children.Append(leftSpacer);
    row.Children.Append(content);
    outer.Children.Append(row);

    return outer;
}

function createShell() {
    console.log("NativeScriptWindowsDemo: createShell start");
    try {
        const navigation = new Windows.UI.Xaml.Controls.NavigationView();
        navigation.PaneTitle = "Demo Studio";
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
    split.OpenPaneLength = 250;

    const pane = new Windows.UI.Xaml.Controls.StackPanel();
    pane.Spacing = 6;
    pane.Children.Append(text("Demo Studio", 26));
    pane.Children.Append(text("Fast native UI from JavaScript", 13));

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

function safeGetChildrenSize(element) {
    try {
        const children = element && element.Children;
        if (children && typeof children.Size === "number") {
            return children.Size;
        }
    } catch (_error) {
    }

    return -1;
}

function createDashboardPage(onNavigate) {
    const root = new Windows.UI.Xaml.Controls.StackPanel();
    root.Spacing = 12;

    const titleView = text("Dashboard", 34);
    const subtitle = text("A polished native shell driven by the Rust runtime and WinRT bindings.", 15);
    subtitle.TextWrapping = Windows.UI.Xaml.TextWrapping.Wrap;

    root.Children.Append(titleView);
    root.Children.Append(subtitle);

    const buttonStatus = text("Button status: waiting for click", 13);
    const clickProbeButton = createButton("Dashboard button click probe", function () {
        buttonStatus.Text = "Button status: click received";
        console.log("NativeScriptWindowsDemo: dashboard button clicked");
    });

    const navigateStatus = text("Navigation probe: idle", 13);
    const navigateProbeButton = createButton("Navigate to Diagnostics (button)", function () {
        navigateStatus.Text = "Navigation probe: attempting navigation";
        console.log("NativeScriptWindowsDemo: dashboard navigation probe button clicked");
        if (typeof onNavigate === "function") {
            onNavigate("logs");
        }
    });

    root.Children.Append(clickProbeButton);
    root.Children.Append(buttonStatus);
    root.Children.Append(navigateProbeButton);
    root.Children.Append(navigateStatus);

    root.Children.Append(
        createCard(
            "Visual Shell",
            "NavigationView navigation, clean hierarchy, and card-based content to make the sample feel like a real app.",
        ),
    );
    root.Children.Append(
        createCard(
            "Interop Health",
            "Enum marshaling, event delegates, and object boxing are active in this build. UI interactions should now remain stable.",
        ),
    );

    console.log("NativeScriptWindowsDemo: dashboard children", safeGetChildrenSize(root));

    return root;
}

function createLogsPage() {
    const root = new Windows.UI.Xaml.Controls.StackPanel();
    root.Spacing = 12;

    root.Children.Append(text("Diagnostics", 34));

    const body = text(
        "Runtime logs are wired to the diagnostics console. Pointer fallback warnings are now filtered for valid WinRT reference types.",
        14,
    );
    body.TextWrapping = Windows.UI.Xaml.TextWrapping.Wrap;
    root.Children.Append(body);

    root.Children.Append(
        createCard(
            "Tip",
            "If something regresses, capture the first failing runtime line and we can trace straight to the invocation path.",
        ),
    );

    console.log("NativeScriptWindowsDemo: logs children", safeGetChildrenSize(root));

    return root;
}

function createSettingsPage() {
    const root = new Windows.UI.Xaml.Controls.StackPanel();
    root.Spacing = 10;

    root.Children.Append(text("Settings", 34));
    root.Children.Append(text("Tune the demo behavior using native WinRT controls.", 14));

    const card = new Windows.UI.Xaml.Controls.Border();

    const cardStack = new Windows.UI.Xaml.Controls.StackPanel();
    cardStack.Spacing = 10;

    const toggle = new Windows.UI.Xaml.Controls.ToggleSwitch();
    toggle.Header = "Enable smooth transitions";
    toggle.IsOn = true;

    const compact = new Windows.UI.Xaml.Controls.ToggleSwitch();
    compact.Header = "Compact navigation mode";
    compact.IsOn = false;

    cardStack.Children.Append(toggle);
    cardStack.Children.Append(compact);
    card.Child = cardStack;

    root.Children.Append(card);
    console.log("NativeScriptWindowsDemo: settings children", safeGetChildrenSize(root));
    return root;
}

function buildApp() {
    console.log("NativeScriptWindowsDemo: buildApp start");

    const shell = createShell();
    const navItemsByKey = {};

    const navigationProbe = function (key) {
        console.log("NativeScriptWindowsDemo: button-triggered direct page route", key);
        showPage(key);
    };

    const pages = {
        dashboard: withContentPadding(createDashboardPage(navigationProbe)),
        logs: withContentPadding(createLogsPage()),
        settings: withContentPadding(createSettingsPage()),
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

        const dashboardItem = createNavigationItem(
            "dashboard",
            "Dashboard",
            "\uE80F",
        );
        const logsItem = createNavigationItem(
            "logs",
            "Diagnostics",
            "\uE7BA",
        );
        const settingsItem = createNavigationItem(
            "settings",
            "Settings",
            "\uE713",
        );

        navItemsByKey.dashboard = dashboardItem;
        navItemsByKey.logs = logsItem;
        navItemsByKey.settings = settingsItem;

        shell.navigation.MenuItems.Append(dashboardItem);
        shell.navigation.MenuItems.Append(logsItem);
        shell.navigation.MenuItems.Append(settingsItem);

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

        const onSelectionChanged = function () {
            const key = resolveNavigationKey();

            if (key && pages[key]) {
                showPage(key);
            }
        };

        shell.navigation.SelectionChanged = onSelectionChanged;
    } else {
        shell.pane.Children.Append(
            createButton("Dashboard", function () {
                showPage("dashboard");
            }),
        );
        shell.pane.Children.Append(
            createButton("Diagnostics", function () {
                showPage("logs");
            }),
        );
        shell.pane.Children.Append(
            createButton("Settings", function () {
                showPage("settings");
            }),
        );
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
