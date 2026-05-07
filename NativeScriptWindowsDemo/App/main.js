console.log("NativeScriptWindowsDemo: booting JS UI demo");

function createSectionTitle(text) {
	const title = new Windows.UI.Xaml.Controls.TextBlock();
	title.Text = text;
	title.FontSize = 18;
	title.Margin = new Windows.UI.Xaml.Thickness(0, 16, 0, 8);
	return title;
}

function createControlCard() {
	const border = new Windows.UI.Xaml.Controls.Border();
	border.BorderBrush = new Windows.UI.Xaml.Media.SolidColorBrush(Windows.UI.Colors.LightGray);
	border.BorderThickness = new Windows.UI.Xaml.Thickness(1);
	border.Padding = new Windows.UI.Xaml.Thickness(12);
	border.CornerRadius = new Windows.UI.Xaml.CornerRadius(8);

	const stack = new Windows.UI.Xaml.Controls.StackPanel();
	stack.Spacing = 8;
	border.Child = stack;
	return { border, stack };
}

function buildDemoLayout() {
	const rootScroll = new Windows.UI.Xaml.Controls.ScrollViewer();
	rootScroll.VerticalScrollBarVisibility = Windows.UI.Xaml.Controls.ScrollBarVisibility.Auto;

	const content = new Windows.UI.Xaml.Controls.StackPanel();
	content.Margin = new Windows.UI.Xaml.Thickness(24, 20, 24, 20);
	content.Spacing = 10;
	rootScroll.Content = content;

	const header = new Windows.UI.Xaml.Controls.TextBlock();
	header.Text = "NativeScript Windows Demo";
	header.FontSize = 30;
	header.FontWeight = Windows.UI.Text.FontWeights.SemiBold;
	content.Children.Append(header);

	const subtitle = new Windows.UI.Xaml.Controls.TextBlock();
	subtitle.Text = "WinRT XAML controls created directly from JavaScript";
	subtitle.FontSize = 14;
	subtitle.Foreground = new Windows.UI.Xaml.Media.SolidColorBrush(Windows.UI.Colors.DimGray);
	subtitle.Margin = new Windows.UI.Xaml.Thickness(0, 0, 0, 6);
	content.Children.Append(subtitle);

	content.Children.Append(createSectionTitle("Inputs"));
	const inputCard = createControlCard();

	const nameBox = new Windows.UI.Xaml.Controls.TextBox();
	nameBox.Header = "Display name";
	nameBox.PlaceholderText = "Type your name";

	const password = new Windows.UI.Xaml.Controls.PasswordBox();
	password.Header = "Password";

	const toggle = new Windows.UI.Xaml.Controls.ToggleSwitch();
	toggle.Header = "Enable notifications";
	toggle.IsOn = true;

	inputCard.stack.Children.Append(nameBox);
	inputCard.stack.Children.Append(password);
	inputCard.stack.Children.Append(toggle);
	content.Children.Append(inputCard.border);

	content.Children.Append(createSectionTitle("Selection"));
	const selectionCard = createControlCard();

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

	slider.ValueChanged = function (sender) {
		progress.Value = sender.Value;
	};

	selectionCard.stack.Children.Append(combo);
	selectionCard.stack.Children.Append(sliderLabel);
	selectionCard.stack.Children.Append(slider);
	selectionCard.stack.Children.Append(progress);
	content.Children.Append(selectionCard.border);

	content.Children.Append(createSectionTitle("Actions"));
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
	status.Margin = new Windows.UI.Xaml.Thickness(0, 6, 0, 0);

	applyButton.Click = function () {
		const selectedTheme = combo.SelectedItem ? String(combo.SelectedItem) : "Unknown";
		status.Text = "Saved for " + (nameBox.Text || "Guest") + " with " + selectedTheme + " theme";
	};

	resetButton.Click = function () {
		nameBox.Text = "";
		password.Password = "";
		toggle.IsOn = true;
		combo.SelectedIndex = 0;
		slider.Value = 72;
		status.Text = "Reset complete";
	};

	actionRow.Children.Append(applyButton);
	actionRow.Children.Append(resetButton);
	actionCard.stack.Children.Append(actionRow);
	actionCard.stack.Children.Append(status);
	content.Children.Append(actionCard.border);

	content.Children.Append(createSectionTitle("Data List"));
	const listCard = createControlCard();
	const list = new Windows.UI.Xaml.Controls.ListView();
	list.Height = 180;
	list.Items.Append("Dashboard");
	list.Items.Append("Analytics");
	list.Items.Append("Notifications");
	list.Items.Append("Reports");
	list.Items.Append("Settings");
	listCard.stack.Children.Append(list);
	content.Children.Append(listCard.border);

	return rootScroll;
}

try {
	const window = Windows.UI.Xaml.Window.Current;
	window.Content = buildDemoLayout();
	window.Activate();
	console.log("NativeScriptWindowsDemo: layout created");
} catch (error) {
	console.log("NativeScriptWindowsDemo: failed to build layout", error && error.message ? error.message : error);
	throw error;
}
