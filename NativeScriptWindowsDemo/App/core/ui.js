const delegateKeepAlive = [];

export function resolveType(typeName) {
    const parts = String(typeName).split(".");
    let current = globalThis;
    for (let i = 0; i < parts.length; i += 1) {
        current = current && current[parts[i]];
    }
    return current;
}

export function text(value, size) {
    const control = new Windows.UI.Xaml.Controls.TextBlock();
    control.Text = String(value);
    if (size) {
        control.FontSize = size;
    }
    return control;
}

export function createTypedDelegate(typeName, callback) {
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

export function bindClick(button, onClick) {
    const callback = typeof onClick === "function" ? onClick : function () {};
    const delegate = createTypedDelegate("Windows.UI.Xaml.RoutedEventHandler", callback);

    if (delegate && typeof button.add_Click === "function") {
        button.add_Click(delegate);
        return;
    }

    button.Click = delegate || callback;
}

export function createButton(label, onClick) {
    const button = new Windows.UI.Xaml.Controls.Button();
    button.Height = 42;
    button.Content = String(label);
    bindClick(button, onClick);
    return button;
}

export function createCard(titleValue, bodyValue) {
    const border = new Windows.UI.Xaml.Controls.Border();

    const stack = new Windows.UI.Xaml.Controls.StackPanel();
    stack.Spacing = 8;

    const titleText = text(titleValue, 19);
    const bodyText = text(bodyValue, 14);
    bodyText.TextWrapping = Windows.UI.Xaml.TextWrapping.Wrap;

    stack.Children.Append(titleText);
    stack.Children.Append(bodyText);

    border.Child = stack;
    return border;
}

export function withContentPadding(content) {
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

export function safeGetChildrenSize(element) {
    try {
        const children = element && element.Children;
        if (children && typeof children.Size === "number") {
            return children.Size;
        }
    } catch (_error) {
    }

    return -1;
}

export function createBrandImage(width, height) {
    const host = new Windows.UI.Xaml.Controls.StackPanel();
    host.Spacing = 0;

    const image = new Windows.UI.Xaml.Controls.Image();
    image.Width = width || 280;
    image.Height = height || 120;
    image.Stretch = Windows.UI.Xaml.Media.Stretch.Uniform;

    const fallback = text("NativeScript", 44);
    fallback.Visibility = Windows.UI.Xaml.Visibility.Collapsed;

    host.Children.Append(image);
    host.Children.Append(fallback);
    
    const candidateUris = [
        "ms-appx:///Assets/Wide310x150Logo.scale-200.png",
        "ms-appx:///Assets/Square150x150Logo.scale-200.png",
        "ms-appx:///Assets/StoreLogo.png",
        "ms-appx:///Assets/cover.png",
        "https://raw.githubusercontent.com/NativeScript/NativeScript/main/tools/graphics/cover.png",
    ].filter(function (value) {
        return typeof value === "string" && value.length > 0;
    });

    let assigned = false;

    const tryAssignImageSource = function () {
        for (let i = 0; i < candidateUris.length; i += 1) {
            const uriValue = candidateUris[i];
            console.log("NativeScriptWindowsDemo: trying image URI", uriValue);

            try {
                const uri = new Windows.Foundation.Uri(uriValue);
                const bitmap = new Windows.UI.Xaml.Media.Imaging.BitmapImage();
                bitmap.UriSource = uri;
                image.Source = bitmap;
                console.log("NativeScriptWindowsDemo: image source assigned", uriValue);
                assigned = true;
                return true;
            } catch (error) {
                console.log("NativeScriptWindowsDemo: invalid image URI", uriValue, error && error.message ? error.message : error);
            }
        }

        return false;
    };

    const showTextFallback = function () {
        image.Visibility = Windows.UI.Xaml.Visibility.Collapsed;
        fallback.Visibility = Windows.UI.Xaml.Visibility.Visible;
    };

    if (!tryAssignImageSource()) {
        showTextFallback();
    }

    if (!assigned) {
        showTextFallback();
    }

    return host;
}

export function openExternalUrl(url) {
    try {
        const launchOp = Windows.System.Launcher.LaunchUriAsync(new Windows.Foundation.Uri(String(url)));
        if (launchOp && typeof NSWinRT !== "undefined" && typeof NSWinRT.onCompleted === "function") {
            NSWinRT.onCompleted(launchOp, function (_asyncInfo, status) {
                console.log("NativeScriptWindowsDemo: launch status", status, url);
            }, { timeoutMs: 12000 });
        }
    } catch (error) {
        console.log("NativeScriptWindowsDemo: failed to open URL", url, error && error.message ? error.message : error);
    }
}
