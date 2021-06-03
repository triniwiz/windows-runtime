console.log("me");

console.log(Date.now(), time(), performance.now());

const MessageDialog = $("Windows.UI.Popups.MessageDialog");
const dialog = new MessageDialog("Hello, World!");
dialog.ShowAsync();