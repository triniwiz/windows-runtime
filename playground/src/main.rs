mod interop;

use std::ffi::CString;
use windows::Win32::System::WinRT::{RO_INIT_SINGLETHREADED, RoInitialize};
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG, TranslateMessage};
use metadata::meta_data_reader::MetadataReader;
use crate::interop::{create_dispatcher_queue_controller_for_current_thread, shutdown_dispatcher_queue_controller_and_exit};

use windows::{
    core::Result
};
use metadata::declarations::declaration::Declaration;


fn run() -> Result<()> {
    unsafe { RoInitialize(RO_INIT_SINGLETHREADED)? };
    let controller = create_dispatcher_queue_controller_for_current_thread()?;

    // MetadataReader::find_by_name("Windows.UI.Popups.MessageDialog");
    let meta = MetadataReader::find_by_name("Windows.UI.Popups.Placement");
    let meta = meta.unwrap();
    let dec = meta.read();
    let dec = dec.as_any().downcast_ref::<metadata::declarations::enum_declaration::EnumDeclaration>();
    //MetadataReader::find_by_name("Windows.UI.Popups");


    if let Some(dec) = dec {
        println!("name {:?}, kind {:?}", dec.full_name(), dec.kind());
        println!("enum count {:?}", dec.size());
        println!("signature {:?}", dec.type_());

        for item in dec.enums() {
            println!("name: {:?}, kind: {:?}, value {:?}", item.full_name(), item.kind(), item.value());
        }

        /* for child in dec.children() {
           let meta = MetadataReader::find_by_name(child);
           let meta = meta.unwrap();
           let dec = meta.read();
           match dec.kind() {
              DeclarationKind::Namespace => {
                 let dec = dec.as_any().downcast_ref::<metadata::declarations::namespace_declaration::NamespaceDeclaration>();
                 dbg!("top level namespace {:?}", dec);
              }
              DeclarationKind::Class => {}
              DeclarationKind::Interface => {}
              DeclarationKind::GenericInterface => {}
              DeclarationKind::GenericInterfaceInstance => {}
              DeclarationKind::Enum => {}
              DeclarationKind::EnumMember => {}
              DeclarationKind::Struct => {}
              DeclarationKind::StructField => {}
              DeclarationKind::Delegate => {}
              DeclarationKind::GenericDelegate => {}
              DeclarationKind::GenericDelegateInstance => {}
              DeclarationKind::Event => {}
              DeclarationKind::Property => {}
              DeclarationKind::Method => {}
              DeclarationKind::Parameter => {}
           }
        } */
    }


    let mut message = MSG::default();
    unsafe {
        while GetMessageW(&mut message, None, 0, 0).into() {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    shutdown_dispatcher_queue_controller_and_exit(&controller, message.wParam.0 as i32);
}

fn run_js_app() {
    let script = r#"
   console.log('Hello From NativeScript running in a Windows CLI App\n');
   console.log(performance.now() + '\n');
   console.dir(global + '\n');
   //console.log(Windows.UI.Xaml);
   console.log('Default', Windows.UI.Popups.Placement.Default, Windows.UI.Popups.Placement.Default === 0);
    console.log('Right', Windows.UI.Popups.Placement.Right, Windows.UI.Popups.Placement.Right === 4);
     console.log('Bar', Windows.UI.Text.TabAlignment.Bar, Windows.UI.Text.TabAlignment.Bar == 4);
    const dialog = new Windows.UI.Popups.MessageDialog("Hello, World!");
  //  dialog.ShowAsync();
  // console.log('Windows.UI.Popups.MessageDialog', dialog);
  //const json = new Windows.Data.Json.JsonObject();
  ///const method = new Windows.Web.Http.HttpMethod('GET');
 // console.log(method);
   console.log("\n");
   "#;
    let rt = nativescript::runtime_init(0 as _);
    let script = CString::new(script).unwrap();
    nativescript::runtime_runscript(rt, script.as_ptr());
    nativescript::runtime_deinit(rt);
}

fn main() {
    run_js_app();
    /*
    let result = run();

    // We do this for nicer HRESULT printing when errors occur.
    if let Err(error) = result {
       error.code().unwrap();
    }
    */


    // MetadataReader::find_by_name("");
}
