const page = new Windows.UI.Xaml.Controls.Page();
const text = new Windows.UI.Xaml.Controls.TextBlock();
text.Text = "Hello from NativeScript on Windows!";
text.FontSize = 24;
text.HorizontalAlignment = Windows.UI.Xaml.HorizontalAlignment.Center;
text.VerticalAlignment = Windows.UI.Xaml.VerticalAlignment.Center;

const grid = new Windows.UI.Xaml.Controls.Grid();
grid.Children.Append(text);
page.Content = grid;

Windows.UI.Xaml.Window.Current.Content = page;
