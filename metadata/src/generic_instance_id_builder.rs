use std::ffi::c_void;
use std::mem::MaybeUninit;
use windows::core::{GUID, HRESULT, HSTRING, IntoParam, PCWSTR};
use windows::Win32::System::WinRT::Metadata::{IRoMetaDataLocator, IRoMetaDataLocator_Impl, IRoSimpleMetaDataBuilder, RoGetParameterizedTypeInstanceIID, RoParseTypeName};
use windows::Win32::System::WinRT::WindowsGetStringRawBuffer;
use crate::declarations::declaration::Declaration;
use crate::prelude::*;

pub struct GenericInstanceIdBuilder {}

pub struct IRoMetaDataLocatorImpl {}

impl IRoMetaDataLocator_Impl for IRoMetaDataLocatorImpl {
    fn Locate(&self, nameelement: &PCWSTR, metadatadestination: Option<&IRoSimpleMetaDataBuilder>) -> windows::core::Result<()> {
        /*let declaration = MetadataReader::find_by_name_w(name);
       debug_assert!(declaration.is_some());

       match Option::as_ref(&declaration) {
           None => {}
           Some(declaration) => match declaration.try_read() {
               Ok(declaration) => match declaration.kind() {
                   DeclarationKind::Class => {
                       let mut class_declaration = declaration
                           .as_any()
                           .downcast_ref::<ClassDeclaration>()
                           .unwrap();

                       let default_interface = class_declaration.default_interface();
                       let default_interface_id = default_interface.id();
                       let full_name = windows::HSTRING::from(default_interface.full_name());
                       let full_name_w = full_name.as_wide();
                       let result = iro_simple_meta_data_builder::set_runtime_class_simple_default(
                           builder,
                           name,
                           full_name_w.as_ptr(),
                           &default_interface_id,
                       );
                       debug_assert!(result.is_ok());
                       return 0;
                   }
                   DeclarationKind::Interface => {
                       let mut interface_declaration = declaration
                           .as_any()
                           .downcast_ref::<InterfaceDeclaration>()
                           .unwrap();
                       let interface_declaration_id = interface_declaration.id();
                       iro_simple_meta_data_builder::set_win_rt_interface(
                           builder,
                           interface_declaration_id,
                       );
                       return 0;
                   }
                   DeclarationKind::GenericInterface => {
                       let mut generic_interface_declaration = declaration
                           .as_any()
                           .downcast_ref::<GenericInterfaceDeclaration>()
                           .unwrap();
                       let result = iro_simple_meta_data_builder::set_parameterized_interface(
                           builder,
                           generic_interface_declaration.id(),
                           generic_interface_declaration.number_of_generic_parameters() as u32,
                       );
                       debug_assert!(result.is_ok());
                       return 0;
                   }
                   DeclarationKind::Enum => {
                       let mut enum_declaration = declaration
                           .as_any()
                           .downcast_ref::<EnumDeclaration>()
                           .unwrap();
                       let type_ = enum_declaration.type_().into_owned();
                       let full_name = windows::HSTRING::from(
                           enum_declaration.full_name().to_owned().as_ref(),
                       );
                       let full_name_w = full_name.as_wide();
                       let signature = Signature::to_string(std::ptr::null() as _, type_.as_ptr());
                       let signature = windows::HSTRING::from(signature.to_owned().as_ref());
                       let signature_w = signature.as_wide();
                       let result = iro_simple_meta_data_builder::set_enum(
                           builder,
                           full_name_w.as_ptr(),
                           signature_w.as_ptr(),
                       );
                       debug_assert!(result.is_ok());
                       return 0;
                   }
                   DeclarationKind::Struct => {
                       let mut struct_declaration =
                           declaration.downcast_ref::<StructDeclaration>().unwrap();

                       let mut field_names = Vec::new();
                       for field in struct_declaration.fields().iter() {
                           let field_type = field.type_().to_owned();
                           let signature =
                               Signature::to_string(field.base().metadata, field_type.as_ptr());
                           let signature = windows::HSTRING::from(signature.as_ref());

                           field_names.push(signature);
                       }

                       let full_name =
                           windows::HSTRING::from(struct_declaration.full_name().as_ref());
                       let full_name_w = full_name.as_wide();
                       let field_names = field_names
                           .into_iter()
                           .map(|field| field.as_wide().as_ptr())
                           .collect();
                       let result = iro_simple_meta_data_builder::set_struct(
                           builder,
                           full_name_w.as_ptr(),
                           struct_declaration.size() as u32,
                           field_names.as_ptr(),
                       );
                       debug_assert!(result.is_ok());

                       return 0;
                   }
                   DeclarationKind::Delegate => {
                       let mut delegate_declaration = declaration
                           .as_any()
                           .downcast_ref::<DelegateDeclaration>()
                           .unwrap();
                       let result = iro_simple_meta_data_builder::set_delegate(
                           builder,
                           delegate_declaration.id(),
                       );
                       debug_assert!(result.is_ok());
                       return 0;
                   }
                   DeclarationKind::GenericDelegate => {
                       let mut generic_delegate_declaration = declaration
                           .as_any()
                           .downcast_ref::<GenericDelegateDeclaration>()
                           .unwrap();
                       debug_assert!(iro_simple_meta_data_builder::set_parameterized_delegate(
                           builder,
                           generic_delegate_declaration.id(),
                           generic_delegate_declaration.number_of_generic_parameters() as u32,
                       )
                           .is_ok());
                       return 0;
                   }
                   _ => {}
               },
               Err(_) => {}
           },
       }
       unreachable!()
       */
        println!("locator_impl");
        windows::core::Result::Ok(())
    }
}


impl GenericInstanceIdBuilder {

    pub fn generate_id(declaration: &dyn Declaration) -> GUID {
        let mut declaration_full_name = declaration.full_name();

        let type_name = HSTRING::from(declaration_full_name);
        let mut parts_count = 0_u32;
        let mut type_name_parts = std::ptr::null_mut();
        let mut type_name_parts_ptr = &mut type_name_parts;
        let _ = unsafe { RoParseTypeName(&type_name, &mut parts_count, type_name_parts_ptr) };

        let mut buf: Vec<PCWSTR> = Vec::with_capacity(parts_count as usize);

        let name_parts = unsafe{std::slice::from_raw_parts(type_name_parts, parts_count as usize)};

        for part in name_parts.iter() {
            buf.push(unsafe { WindowsGetStringRawBuffer(part, None)})
        }


        let mut guid = GUID::zeroed();


        let locator = IRoMetaDataLocatorImpl {};
        let locator = IRoMetaDataLocator::new(&locator);
        let locator = &*locator;

        let result = unsafe { RoGetParameterizedTypeInstanceIID(buf.as_slice(), locator, &mut guid, None) };

        assert!(result.is_ok());

        return guid;
    }
}