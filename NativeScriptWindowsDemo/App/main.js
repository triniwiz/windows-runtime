console.log("NativeScriptWindowsDemo: booting JS UI demo");

let layoutTraceLogFile = null;

function traceLayoutStep(step) {
	const message = "NativeScriptWindowsDemo: layout step -> " + step;
	console.log(message);

	try {
		if (!layoutTraceLogFile) {
			const folder = Windows.Storage.ApplicationData.Current.LocalFolder;
			const createLogFile = folder.CreateFileAsync(
				"nativescript-layout-trace.log",
				Windows.Storage.CreationCollisionOption.OpenIfExists
			);
			layoutTraceLogFile = typeof NSWinRT !== "undefined" && NSWinRT.wait
				? NSWinRT.wait(createLogFile)
				: createLogFile;
		}

		const appendTrace = Windows.Storage.FileIO.AppendTextAsync(layoutTraceLogFile, message + "\n");
		if (typeof NSWinRT !== "undefined" && NSWinRT.wait) {
			NSWinRT.wait(appendTrace);
		}
	} catch (_) {
		// Best effort only; layout tracing must never become the failure.
	}
}

function thickness(left, top, right, bottom) {
	return new Windows.UI.Xaml.Thickness({
		Left: left,
		Top: top,
		Right: right,
		Bottom: bottom,
	});
}

function uniformThickness(value) {
	return thickness(value, value, value, value);
}

function uniformCornerRadius(value) {
	return new Windows.UI.Xaml.CornerRadius({
		TopLeft: value,
		TopRight: value,
		BottomRight: value,
		BottomLeft: value,
	});
}

function createSectionTitle(text) {
	const title = new Windows.UI.Xaml.Controls.TextBlock();
	title.Text = text;
	title.FontSize = 18;
	title.Margin = thickness(0, 16, 0, 8);
	return title;
}

function createControlCard() {
	const border = new Windows.UI.Xaml.Controls.Border();
	border.BorderBrush = new Windows.UI.Xaml.Media.SolidColorBrush(Windows.UI.Colors.LightGray);
	border.BorderThickness = uniformThickness(1);
	border.Padding = uniformThickness(12);
	border.CornerRadius = uniformCornerRadius(8);

	const stack = new Windows.UI.Xaml.Controls.StackPanel();
	stack.Spacing = 8;
	border.Child = stack;
	return { border, stack };
}

function buildDemoLayout() {
	traceLayoutStep("create root scroll viewer");
	const rootScroll = new Windows.UI.Xaml.Controls.ScrollViewer();
	console.log("rootScroll", rootScroll);
	traceLayoutStep("set root scroll viewer properties");
	rootScroll.VerticalScrollBarVisibility = Windows.UI.Xaml.Controls.ScrollBarVisibility.Auto;

	traceLayoutStep("create content stack panel");
	const content = new Windows.UI.Xaml.Controls.StackPanel();
	traceLayoutStep("set content stack panel properties");
	content.Margin = thickness(24, 20, 24, 20);
	content.Spacing = 10;
	traceLayoutStep("attach content stack panel to root scroll viewer");
	rootScroll.Content = content;

	traceLayoutStep("create header text block");
	const header = new Windows.UI.Xaml.Controls.TextBlock();
	header.Text = "NativeScript Windows Demo";
	header.FontSize = 30;
	header.FontWeight = Windows.UI.Text.FontWeights.SemiBold;
	traceLayoutStep("append header text block");
	content.Children.Append(header);

	traceLayoutStep("create subtitle text block");
	const subtitle = new Windows.UI.Xaml.Controls.TextBlock();
	subtitle.Text = "WinRT XAML controls created directly from JavaScript";
	subtitle.FontSize = 14;
	subtitle.Foreground = new Windows.UI.Xaml.Media.SolidColorBrush(Windows.UI.Colors.DimGray);
	subtitle.Margin = thickness(0, 0, 0, 6);
	traceLayoutStep("append subtitle text block");
	content.Children.Append(subtitle);

	traceLayoutStep("append inputs section title");
	content.Children.Append(createSectionTitle("Inputs"));
	traceLayoutStep("create input card");
	const inputCard = createControlCard();

	traceLayoutStep("create input controls");
	const nameBox = new Windows.UI.Xaml.Controls.TextBox();
	nameBox.Header = "Display name";
	nameBox.PlaceholderText = "Type your name";

	const password = new Windows.UI.Xaml.Controls.PasswordBox();
	password.Header = "Password";

	const toggle = new Windows.UI.Xaml.Controls.ToggleSwitch();
	toggle.Header = "Enable notifications";
	toggle.IsOn = true;

	traceLayoutStep("append input controls");
	inputCard.stack.Children.Append(nameBox);
	inputCard.stack.Children.Append(password);
	inputCard.stack.Children.Append(toggle);
	traceLayoutStep("append input card to content");
	content.Children.Append(inputCard.border);

	traceLayoutStep("append selection section title");
	content.Children.Append(createSectionTitle("Selection"));
	traceLayoutStep("create selection card");
	const selectionCard = createControlCard();

	traceLayoutStep("create combo box and items");
	const combo = new Windows.UI.Xaml.Controls.ComboBox();
	combo.Header = "Theme";
	combo.Items.Append("Ocean");
	combo.Items.Append("Forest");
	combo.Items.Append("Sunset");
	combo.SelectedIndex = 0;

	const sliderLabel = new Windows.UI.Xaml.Controls.TextBlock();
	sliderLabel.Text = "Volume";
	const slider = new Windows.UI.Xaml.Controls.Slider();
	slider.Minimum = 0;
	slider.Maximum = 100;
	slider.Value = 72;

	const progress = new Windows.UI.Xaml.Controls.ProgressBar();
	progress.Minimum = 0;
	progress.Maximum = 100;
	progress.Value = slider.Value;
	progress.Height = 8;

	traceLayoutStep("wire slider value changed handler");
	slider.ValueChanged = new Windows.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventHandler(function(sender, args) {
		progress.Value = sender.Value;
	});

	traceLayoutStep("append selection controls");
	selectionCard.stack.Children.Append(combo);
	selectionCard.stack.Children.Append(sliderLabel);
	selectionCard.stack.Children.Append(slider);
	selectionCard.stack.Children.Append(progress);
	traceLayoutStep("append selection card to content");
	content.Children.Append(selectionCard.border);

	traceLayoutStep("append actions section title");
	content.Children.Append(createSectionTitle("Actions"));
	traceLayoutStep("create action card and row");
	const actionCard = createControlCard();
	const actionRow = new Windows.UI.Xaml.Controls.StackPanel();
	actionRow.Orientation = Windows.UI.Xaml.Controls.Orientation.Horizontal;
	actionRow.Spacing = 10;

	const applyButton = new Windows.UI.Xaml.Controls.Button();
	applyButton.Content = "Apply";
	applyButton.MinWidth = 120;

	const resetButton = new Windows.UI.Xaml.Controls.Button();
	resetButton.Content = "Reset";
	resetButton.MinWidth = 120;

	const status = new Windows.UI.Xaml.Controls.TextBlock();
	status.Text = "Ready";
	status.Margin = thickness(0, 6, 0, 0);

	applyButton.Click = new Windows.UI.Xaml.RoutedEventHandler(function(sender, args) {
		const selectedTheme = combo.SelectedItem ? String(combo.SelectedItem) : "Unknown";
		status.Text = "Saved for " + (nameBox.Text || "Guest") + " with " + selectedTheme + " theme";
	});

	resetButton.Click = new Windows.UI.Xaml.RoutedEventHandler(function(sender, args) {
		nameBox.Text = "";
		password.Password = "";
		toggle.IsOn = true;
		combo.SelectedIndex = 0;
		slider.Value = 72;
		status.Text = "Reset complete";
	});

	traceLayoutStep("append action controls");
	actionRow.Children.Append(applyButton);
	actionRow.Children.Append(resetButton);
	actionCard.stack.Children.Append(actionRow);
	actionCard.stack.Children.Append(status);
	traceLayoutStep("append action card to content");
	content.Children.Append(actionCard.border);

	traceLayoutStep("append data list section title");
	content.Children.Append(createSectionTitle("Data List"));
	traceLayoutStep("create list card and list view");
	const listCard = createControlCard();
	const list = new Windows.UI.Xaml.Controls.ListView();
	list.Height = 180;
	traceLayoutStep("append list items");
	list.Items.Append("Dashboard");
	list.Items.Append("Analytics");
	list.Items.Append("Notifications");
	list.Items.Append("Reports");
	list.Items.Append("Settings");
	traceLayoutStep("append list view to card");
	listCard.stack.Children.Append(list);
	traceLayoutStep("append list card to content");
	content.Children.Append(listCard.border);

	// ── WinRT APIs ────────────────────────────────────────────────────────────
	// Demonstrates three built-in WinRT namespaces callable directly from JS:
	//   1. Windows.Globalization.Calendar  – read current local date/time
	//   2. Windows.Web.Http.HttpClient     – async HTTP GET with NSWinRT.wait()
	//   3. Windows.ApplicationModel.DataTransfer.Clipboard – write to clipboard

	traceLayoutStep("append WinRT APIs section title");
	content.Children.Append(createSectionTitle("WinRT APIs"));
	traceLayoutStep("create WinRT APIs card");
	const apiCard = createControlCard();

	// 1. Calendar — current local date/time (no async, pure value getters)
	const calendar = new Windows.Globalization.Calendar();
	const nowBlock = new Windows.UI.Xaml.Controls.TextBlock();
	nowBlock.Text = "Local time: "
		+ calendar.MonthAsNumericString() + "/"
		+ calendar.DayAsString() + "/"
		+ calendar.YearAsString()
		+ "  "
		+ calendar.HourAsPaddedString(2) + ":"
		+ calendar.MinuteAsPaddedString(2);
	apiCard.stack.Children.Append(nowBlock);

	// 2. HttpClient — async GET, result shown on click
	const httpRow = new Windows.UI.Xaml.Controls.StackPanel();
	httpRow.Orientation = Windows.UI.Xaml.Controls.Orientation.Horizontal;
	httpRow.Spacing = 10;

	const fetchButton = new Windows.UI.Xaml.Controls.Button();
	fetchButton.Content = "HTTP GET httpbin.org";

	const httpStatus = new Windows.UI.Xaml.Controls.TextBlock();
	httpStatus.Text = "Press to fetch";
	httpStatus.VerticalAlignment = Windows.UI.Xaml.VerticalAlignment.Center;

	httpRow.Children.Append(fetchButton);
	httpRow.Children.Append(httpStatus);
	apiCard.stack.Children.Append(httpRow);

	const httpBody = new Windows.UI.Xaml.Controls.TextBlock();
	httpBody.TextWrapping = Windows.UI.Xaml.TextWrapping.Wrap;
	httpBody.FontSize = 12;
	apiCard.stack.Children.Append(httpBody);

	fetchButton.Click = new Windows.UI.Xaml.RoutedEventHandler(function(sender, args) {
		try {
			httpStatus.Text = "Fetching…";
			const client = new Windows.Web.Http.HttpClient();
			const uri = new Windows.Foundation.Uri("https://httpbin.org/get");
			const response = NSWinRT.wait(client.GetAsync(uri));
			const body = NSWinRT.wait(response.Content.ReadAsStringAsync());
			httpStatus.Text = "HTTP " + response.StatusCode;
			httpBody.Text = body.length > 300 ? body.substring(0, 300) + "…" : body;
		} catch (e) {
			httpStatus.Text = "Error: " + (e && e.message ? e.message : String(e));
		}
	});

	// 3. Clipboard — write a string via DataPackage
	const clipRow = new Windows.UI.Xaml.Controls.StackPanel();
	clipRow.Orientation = Windows.UI.Xaml.Controls.Orientation.Horizontal;
	clipRow.Spacing = 10;

	const copyButton = new Windows.UI.Xaml.Controls.Button();
	copyButton.Content = "Copy to Clipboard";

	const clipStatus = new Windows.UI.Xaml.Controls.TextBlock();
	clipStatus.Text = "Nothing copied yet";
	clipStatus.VerticalAlignment = Windows.UI.Xaml.VerticalAlignment.Center;

	clipRow.Children.Append(copyButton);
	clipRow.Children.Append(clipStatus);
	apiCard.stack.Children.Append(clipRow);

	copyButton.Click = new Windows.UI.Xaml.RoutedEventHandler(function(sender, args) {
		const pkg = new Windows.ApplicationModel.DataTransfer.DataPackage();
		pkg.SetText("Hello from NativeScript Windows!");
		Windows.ApplicationModel.DataTransfer.Clipboard.SetContent(pkg);
		clipStatus.Text = "Copied!";
	});

	traceLayoutStep("append WinRT APIs card to content");
	content.Children.Append(apiCard.border);

	traceLayoutStep("return root scroll viewer");
	return rootScroll;
}

try {
	const window = Windows.UI.Xaml.Window.Current;
//	const layout = buildDemoLayout();
	//console.log("Content",window.Content, layout);
	const btn = new Windows.UI.Xaml.Controls.Button();
	btn.Content = "Tapp Me";
	window.Content = btn;
	console.log(window.Content);
	window.Activate();
	console.log("NativeScriptWindowsDemo: layout created");
} catch (error) {
	console.log("NativeScriptWindowsDemo: failed to build layout", error && error.message ? error.message : error);
	throw error;
}
