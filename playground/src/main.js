console.log("Hello From NativeScript running in a Windows CLI App\n");
console.log(performance.now() + '\n');
console.dir(global + '\n');
console.dir(Windows.UI);
console.log("\n")
const dialog = new Windows.UI.Popups.MessageDialog("Hello, World!");
console.log("Dialog created:", dialog);
const op = dialog.ShowAsync();

NSWinRT.onCompleted(
	op,
	(asyncInfo, asyncStatus) => {
		console.log("Dialog completed with status:", asyncStatus);
		if (asyncStatus === 1) {
			console.log("Dialog result:", NSWinRT.getResults(asyncInfo));
		} else if (asyncStatus === 2) {
			console.log("Dialog canceled");
		} else if (asyncStatus === 3) {
			console.log("Dialog error code:", asyncInfo.ErrorCode);
		}
	},
	{ timeoutMs: 10000 }
);



const newGuid = Windows.Foundation.GuidHelper.CreateNewGuid();
console.log(newGuid);