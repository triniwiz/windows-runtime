use std::ffi::{c_void, OsString};
use std::mem::MaybeUninit;
use std::os::windows::prelude::OsStringExt;
use std::ptr::addr_of_mut;
use windows::core::{GUID, HRESULT, HSTRING, IntoParam, PCWSTR};
use windows::Win32::System::WinRT::Metadata::{IRoMetaDataLocator, IRoMetaDataLocator_Impl, IRoSimpleMetaDataBuilder, RoGetParameterizedTypeInstanceIID, RoParseTypeName};
use windows::Win32::System::WinRT::WindowsGetStringRawBuffer;
use crate::declarations::class_declaration::ClassDeclaration;
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::delegate_declaration::{DelegateDeclaration, DelegateDeclarationImpl};
use crate::declarations::delegate_declaration::generic_delegate_declaration::GenericDelegateDeclaration;
use crate::declarations::enum_declaration::EnumDeclaration;
use crate::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use crate::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use crate::declarations::interface_declaration::InterfaceDeclaration;
use crate::declarations::struct_declaration::StructDeclaration;
use crate::meta_data_reader::MetadataReader;
use crate::prelude::*;
use crate::signature::Signature;

pub struct GenericInstanceIdBuilder {}

#[derive(Clone)]
pub struct IRoMetaDataLocatorImpl;

impl IRoMetaDataLocator_Impl for IRoMetaDataLocatorImpl {
    fn Locate(&self, nameelement: &PCWSTR, metadatadestination: Option<&IRoSimpleMetaDataBuilder>) -> windows::core::Result<()> {
        let name = OsString::from_wide(unsafe { nameelement.as_wide() });
        let name = name.to_string_lossy();

        let declaration = MetadataReader::find_by_name(name.as_ref());

        let name = PCWSTR(nameelement.as_ptr());

        debug_assert!(declaration.is_some());


        match declaration.as_ref() {
            None => {}
            Some(declaration) => {
                let declaration = declaration.read();

                match declaration.kind() {
                    DeclarationKind::Class => {
                        let mut class_declaration = declaration
                            .as_any()
                            .downcast_ref::<ClassDeclaration>()
                            .unwrap();

                        let default_interface = class_declaration.default_interface().unwrap();
                        let default_interface_id = default_interface.id();
                        let full_name = HSTRING::from(default_interface.full_name());
                        let full_name = PCWSTR::from_raw(full_name.as_ptr());

                        if let Some(builder) = metadatadestination {
                            let result = unsafe {
                                builder.SetRuntimeClassSimpleDefault(
                                    name,
                                    full_name,
                                    Some(&default_interface_id),
                                )
                            };

                            debug_assert!(result.is_ok());
                        }

                        return Ok(());
                    }
                    DeclarationKind::Interface => {
                        let mut interface_declaration = declaration
                            .as_any()
                            .downcast_ref::<InterfaceDeclaration>()
                            .unwrap();
                        let interface_declaration_id = interface_declaration.id();

                        if let Some(builder) = metadatadestination {
                            let result = unsafe {
                                builder.SetWinRtInterface(
                                    interface_declaration_id
                                )
                            };

                            debug_assert!(result.is_ok())
                        }
                        return Ok(());
                    }
                    DeclarationKind::GenericInterface => {
                        let mut generic_interface_declaration = declaration
                            .as_any()
                            .downcast_ref::<GenericInterfaceDeclaration>()
                            .unwrap();

                        match metadatadestination {
                            None => {}
                            Some(builder) => {
                                return unsafe {
                                    builder.SetParameterizedInterface(
                                        generic_interface_declaration.id(),
                                        generic_interface_declaration.number_of_generic_parameters() as u32,
                                    )
                                };
                            }
                        }
                        return Ok(());
                    }
                    DeclarationKind::Enum => {
                        let mut enum_declaration = declaration
                            .as_any()
                            .downcast_ref::<EnumDeclaration>()
                            .unwrap();
                        let type_ = enum_declaration.type_();
                        let full_name = HSTRING::from(
                            enum_declaration.full_name()
                        );
                        let signature = Signature::as_string(&type_);
                        let signature = HSTRING::from(signature);

                        if let Some(builder) = metadatadestination {
                            let full_name = PCWSTR(full_name.as_ptr());
                            let signature = PCWSTR(signature.as_ptr());
                            let result = unsafe {
                                builder.SetEnum(
                                    full_name,
                                    signature,
                                )
                            };

                            debug_assert!(result.is_ok());
                        }

                        return Ok(());
                    }
                    DeclarationKind::Struct => {
                        let mut struct_declaration =
                            declaration.as_any().downcast_ref::<StructDeclaration>().unwrap();

                        if let Some(builder) = metadatadestination {
                            let mut field_names = Vec::new();
                            for field in struct_declaration.fields().iter() {
                                let field_type = field.type_();
                                let signature =
                                    Signature::to_string(field.base().metadata().unwrap(), &field_type);

                                let signature = HSTRING::from(signature);

                                field_names.push(signature);
                            }

                            let full_name = HSTRING::from(struct_declaration.full_name());
                            let full_name = PCWSTR::from_raw(full_name.as_ptr());

                            let field_names: Vec<PCWSTR> = field_names
                                .iter()
                                .map(|field| PCWSTR(field.as_ptr()))
                                .collect();


                            let result = unsafe {
                                builder.SetStruct(
                                    full_name,
                                    field_names.as_slice(),
                                )
                            };

                            debug_assert!(result.is_ok());
                        }

                        return Ok(());
                    }
                    DeclarationKind::Delegate => {
                        let mut delegate_declaration = declaration
                            .as_any()
                            .downcast_ref::<DelegateDeclaration>()
                            .unwrap();

                        if let Some(builder) = metadatadestination {
                            let result = unsafe {
                                builder.SetDelegate(
                                    delegate_declaration.id()
                                )
                            };

                            debug_assert!(result.is_ok());
                        }

                        return Ok(());
                    }
                    DeclarationKind::GenericDelegate => {
                        let mut generic_delegate_declaration = declaration
                            .as_any()
                            .downcast_ref::<GenericDelegateDeclaration>()
                            .unwrap();


                        if let Some(builder) = metadatadestination {
                            let result = unsafe {
                                builder.SetParameterizedDelegate(
                                    generic_delegate_declaration.id(),
                                    generic_delegate_declaration.number_of_generic_parameters() as u32,
                                )
                            };


                            debug_assert!(result.is_ok());
                        }

                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
        unreachable!();
    }
}

impl GenericInstanceIdBuilder {
    pub fn generate_id(declaration: &dyn Declaration) -> GUID {
        let mut declaration_full_name = declaration.full_name().to_string();

        let type_name = HSTRING::from(declaration_full_name);
        let mut parts_count = 0_u32;
        let mut type_name_parts = std::ptr::null_mut();

        let _ = unsafe { RoParseTypeName(&type_name, &mut parts_count, addr_of_mut!(type_name_parts)) };

        let mut buf: Vec<PCWSTR> = Vec::with_capacity(parts_count as usize);

        let name_parts = unsafe { std::slice::from_raw_parts(type_name_parts, parts_count as usize) };

        for part in name_parts.iter() {
            buf.push(PCWSTR(part.as_ptr()))
        }

        let mut guid = GUID::zeroed();

        let locator = IRoMetaDataLocatorImpl {};

        let locator = IRoMetaDataLocator::new(&locator);

        let result = unsafe { RoGetParameterizedTypeInstanceIID(buf.as_slice(), &*locator, &mut guid, None) };

        assert!(result.is_ok());

        return guid;
    }
}