mod interop;

use std::ffi::CString;
use windows::Win32::System::WinRT::{RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG, TranslateMessage};
use crate::interop::create_dispatcher_queue_controller_for_current_thread;

fn run_js_app() {
    // Initialize WinRT in STA mode. UI APIs like MessageDialog require an
    // apartment-threaded context, and RO_INIT_SINGLETHREADED creates a proper
    // ASTA so the shell can show dialogs from a console app.
    unsafe { RoInitialize(RO_INIT_SINGLETHREADED).expect("RoInitialize failed") };

    // A DispatcherQueue on this thread is what WinRT async APIs (e.g. ShowAsync)
    // hand their completion callbacks back to. Held alive for the program's life.
    let _controller = create_dispatcher_queue_controller_for_current_thread()
        .expect("DispatcherQueueController creation failed");

    // The runtime returns native WinRT objects directly — including
    // IAsyncOperation. JS code wraps it in a Promise itself, the same way
    // NativeScript-iOS and NativeScript-Android expose native async types.
    let script = r#"
        console.log('--- NativeScript on Windows (console demo) ---');
        console.log('performance.now() =', performance.now());
        console.log('typeof Windows =', typeof Windows);

        // Basic sync API
        const uri = new Windows.Foundation.Uri("http://www.bing.com/");
        console.log('AbsoluteUri:', uri.AbsoluteUri);

        console.dir(Windows.UI.Popups.Placement);
console.log('Default', Windows.UI.Popups.Placement.Default, Windows.UI.Popups.Placement.Default === 0);
console.log('Right', Windows.UI.Popups.Placement.Right, Windows.UI.Popups.Placement.Right === 4);
const json = new Windows.Data.Json.JsonObject();
const method = new Windows.Web.Http.HttpMethod('GET');
console.log(method);

/*
const dialog = new Windows.UI.Popups.MessageDialog("Hello, World!");
console.log("Dialog created:", dialog);
NSWinRT.toPromise(dialog.ShowAsync())
	.then((result) => {
		console.log("Dialog result:", result);
	})
	.catch((err) => {
		console.log("Dialog error:", err);
	});*/

    var newGuid = Windows.Foundation.GuidHelper.CreateNewGuid();
    console.log(newGuid);



    "#;

    let rt = nativescript::runtime_init(std::ptr::null());
    let cscript = CString::new(script).unwrap();
    nativescript::runtime_runscript(rt, cscript.as_ptr());
    nativescript::runtime_deinit(rt);

    unsafe { RoUninitialize() };
}

fn main() {
    nativescript::runtime_install_ctrlc_handler(0);
    run_js_app();
}
