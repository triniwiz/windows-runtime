use crate::prelude::*;


pub(crate) fn ro_resolve_namespace(name: HSTRING,
								   windows_meta_data_dir: Option<HSTRING>,
								   package_graph_dirs_count: DWORD,
								   package_graph_dirs: Option<*const HSTRING>,
								   meta_data_file_paths_count: Option<*mut DWORD>,
								   meta_data_file_paths: Option<*mut *mut HSTRING>,
								   sub_namespaces_count: *mut DWORD,
								   sub_namespaces: Option<*mut *mut HSTRING>,
) -> HRESULT {
	unsafe {
		core_bindings::RoResolveNamespace(
			name, windows_meta_data_dir.unwrap_or(0 as _), package_graph_dirs_count, package_graph_dirs.unwrap_or(0 as _), meta_data_file_paths_count.unwrap_or(0 as _), meta_data_file_paths.unwrap_or(0 as _), sub_namespaces_count, sub_namespaces.unwrap_or(0 as _),
		)
	}
}

pub mod rometadataresolution {
	use crate::prelude::*;
	use windows::{HRESULT, HSTRING};

	pub(crate) fn ro_get_meta_data_file(
		name: &mut windows::HSTRING, meta_data_dispenser: Option<*mut c_void>, meta_data_file_path: Option<*mut c_void>, meta_data_import: Option<*mut *mut c_void>, type_def_token: Option<*mut u32>,
	) -> HRESULT {
		unsafe {
			windows::HRESULT(core_bindings::Rometadataresolution_RoGetMetaDataFile(
				std::mem::transmute(name), meta_data_dispenser.unwrap_or(0 as _), meta_data_file_path.unwrap_or(0 as _), meta_data_import.unwrap_or(0 as _), type_def_token.unwrap_or(0 as _),
			) as u32)
		}
	}

	pub(crate) fn ro_parse_type_name(
		mut type_name: HSTRING,
		parts_count: *mut DWORD,
		type_name_parts: *mut *mut HSTRING,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::Rometadataresolution_RoParseTypeName(
					std::mem::transmute(&mut type_name),
					parts_count,
					std::mem::transmute(type_name_parts),
				) as u32
			)
		}
	}


	pub(crate) fn ro_get_parameterized_type_instance_iid(
		name_element_count: UINT32,
		name_elements: *mut PCWSTR,
		fn_: ::core::option::Option<
			unsafe extern "C" fn(name: PCWSTR, builder: *mut IRoSimpleMetaDataBuilder) -> i32,
		>,
		iid: *mut GUID,
		p_extra: Option<*mut ROPARAMIIDHANDLE>,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::Rometadataresolution_RoGetParameterizedTypeInstanceIID(
					name_element_count,
					name_elements,
					fn_,
					iid,
					p_extra.unwrap_or(0 as _),
				) as u32
			)
		}
	}

	pub(crate) fn ro_locator(
		fn_: ::core::option::Option<
			unsafe extern "C" fn(name: PCWSTR, builder: *mut IRoSimpleMetaDataBuilder) -> i32,
		>,
		locator: *mut Locator,
	) {
		unsafe {
			core_bindings::Rometadataresolution_Ro_Locator(fn_, locator)
		}
	}
}

pub mod imeta_data_import2 {
	use crate::prelude::*;
	use core_bindings::*;
	use windows::HRESULT;

	pub(crate) fn get_type_def_props(metadata: *mut IMetaDataImport2, md_type_def: c_uint, sz_type_def: Option<LPWSTR>, cch_type_def: Option<ULONG>, pch_type_def: Option<*mut ULONG>, pdw_type_def_flags: Option<*mut ULONG>, md_token: Option<*mut ULONG32>) -> HRESULT {
		unsafe {
			HRESULT(core_bindings::IMetaDataImport2_GetTypeDefProps(metadata, md_type_def, sz_type_def.unwrap_or(0 as _), cch_type_def.unwrap_or(0), pch_type_def.unwrap_or(&mut 0), pdw_type_def_flags.unwrap_or(0 as _), md_token.unwrap_or(&mut 0)) as u32)
		}
	}

	fn get_type_def_props_name_size(metadata: *mut IMetaDataImport2, md_type_def: c_uint, pch_type_def: *mut ULONG) -> HRESULT {
		unsafe {
			HRESULT(core_bindings::IMetaDataImport2_GetTypeDefPropsNameSize(metadata, md_type_def, pch_type_def) as u32)
		}
	}

	pub(crate) fn get_field_props(metadata: *mut IMetaDataImport2,
								  mb: mdFieldDef,
								  p_class: Option<*mut mdTypeDef>,
								  sz_field: Option<*mut u16>,
								  cch_field: Option<ULONG>,
								  pch_field: Option<*mut ULONG>,
								  pdw_attr: Option<*mut DWORD>,
								  ppv_sig_blob: Option<*mut PCCOR_SIGNATURE>,
								  pcb_sig_blob: Option<*mut ULONG>,
								  pdw_cplus_type_flag: Option<*mut DWORD>,
								  pp_value: Option<*mut UVCP_CONSTANT>,
								  pcch_value: Option<*mut ULONG>) -> windows::HRESULT {
		unsafe {
			HRESULT(core_bindings::IMetaDataImport2_GetFieldProps(metadata,
																  mb,
																  p_class.unwrap_or(0 as _),
																  sz_field.unwrap_or(0 as _),
																  cch_field.unwrap_or_default(),
																  pch_field.unwrap_or(0 as _),
																  pdw_attr.unwrap_or(0 as _),
																  ppv_sig_blob.unwrap_or(0 as _),
																  pcb_sig_blob.unwrap_or(0 as _),
																  pdw_cplus_type_flag.unwrap_or(0 as _),
																  pp_value.unwrap_or(0 as _),
																  pcch_value.unwrap_or(0 as _)) as u32)
		}
	}

	pub(crate) fn get_property_props(metadata: *mut IMetaDataImport2, prop: mdProperty, p_class: Option<*mut mdTypeDef>,
									 sz_property: Option<*mut u16>,
									 cch_property: Option<ULONG>,
									 pch_property: Option<*mut ULONG>,
									 pdw_prop_flags: Option<*mut DWORD>,
									 ppv_sig: Option<*mut PCCOR_SIGNATURE>,
									 pb_sig: Option<*mut ULONG>,
									 pdw_cplus_type_flag: Option<*mut DWORD>,
									 pp_default_value: Option<*mut UVCP_CONSTANT>,
									 pcch_default_value: Option<*mut ULONG>,
									 pmd_setter: Option<*mut mdMethodDef>,
									 pmd_getter: Option<*mut mdMethodDef>,
									 rmd_other_method: Option<*mut mdMethodDef>,
									 c_max: Option<ULONG>,
									 pc_other_method: Option<*mut ULONG>) -> windows::HRESULT {
		unsafe {
			HRESULT(core_bindings::IMetaDataImport2_GetPropertyProps(
				metadata, prop,
				p_class.unwrap_or(0 as _),
				sz_property.unwrap_or(0 as _),
				cch_property.unwrap_or_default(),
				pch_property.unwrap_or(0 as _),
				pdw_prop_flags.unwrap_or(0 as _),
				ppv_sig.unwrap_or(0 as _),
				pb_sig.unwrap_or(0 as _),
				pdw_cplus_type_flag.unwrap_or(0 as _),
				pp_default_value.unwrap_or(0 as _),
				pcch_default_value.unwrap_or(0 as _),
				pmd_setter.unwrap_or(0 as _),
				pmd_getter.unwrap_or(0 as _),
				rmd_other_method.unwrap_or(0 as _),
				c_max.unwrap_or_default(),
				pc_other_method.unwrap_or(0 as _),
			) as u32)
		}
	}

	pub(crate) fn get_method_props(metadata: *mut IMetaDataImport2,
								   tk_method_def: mdMethodDef,
								   ptk_class: Option<*mut mdTypeDef>,
								   sz_method: Option<*mut u16>,
								   cch_method: Option<ULONG>,
								   pch_method: Option<*mut ULONG>,
								   pdw_attr: Option<*mut DWORD>,
								   ppv_sig_blob: Option<*mut PCCOR_SIGNATURE>,
								   pcb_sig_blob: Option<*mut ULONG>,
								   pul_code_rva: Option<*mut ULONG>,
								   pdw_impl_flags: Option<*mut DWORD>) -> windows::HRESULT {
		unsafe {
			HRESULT(core_bindings::IMetaDataImport2_GetMethodProps(
				metadata, tk_method_def,
				ptk_class.unwrap_or(0 as _), sz_method.unwrap_or(0 as _),
				cch_method.unwrap_or_default(), pch_method.unwrap_or(0 as _),
				pdw_attr.unwrap_or(0 as _), ppv_sig_blob.unwrap_or(0 as _),
				pcb_sig_blob.unwrap_or(0 as _), pul_code_rva.unwrap_or(0 as _),
				pdw_impl_flags.unwrap_or(0 as _),
			) as u32)
		}
	}


	pub(crate) fn enum_params(metadata: *mut IMetaDataImport2, ph_enum: *mut HCORENUM, mb: mdMethodDef, r_params: *mut mdParamDef, c_max: ULONG,
							  pc_tokens: *mut ULONG) -> windows::HRESULT {
		unsafe { HRESULT(core_bindings::IMetaDataImport2_EnumParams(metadata, ph_enum, mb, r_params, c_max, pc_tokens) as u32) }
	}

	pub(crate) fn close_enum(metadata: *mut IMetaDataImport2, ph_enum: HCORENUM) {
		unsafe {
			core_bindings::IMetaDataImport2_CloseEnum(metadata, ph_enum)
		}
	}

	pub(crate) fn get_param_props(metadata: *mut IMetaDataImport2, tk: mdParamDef,
								  pmd: Option<*mut mdMethodDef>,
								  pul_sequence: Option<*mut ULONG>,
								  sz_name: Option<*mut u16>,
								  cch_name: Option<ULONG>,
								  pch_name: Option<*mut ULONG>,
								  pdw_attr: Option<*mut DWORD>,
								  pdw_cplus_type_flag: Option<*mut DWORD>,
								  pp_value: Option<*mut UVCP_CONSTANT>,
								  pcch_value: Option<*mut ULONG>) -> windows::HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IMetaDataImport2_GetParamProps(metadata, tk,
															  pmd.unwrap_or(0 as _),
															  pul_sequence.unwrap_or(0 as _),
															  sz_name.unwrap_or(0 as _),
															  cch_name.unwrap_or_default(),
															  pch_name.unwrap_or(0 as _),
															  pdw_attr.unwrap_or(0 as _),
															  pdw_cplus_type_flag.unwrap_or(0 as _),
															  pp_value.unwrap_or(0 as _),
															  pcch_value.unwrap_or(0 as _)) as u32
			)
		}
	}

	pub(crate) fn get_custom_attribute_by_name(metadata: *mut IMetaDataImport2,
											   tk_obj: mdToken,
											   sz_name: Option<*const u16>,
											   pp_data: Option<*mut *const ::core::ffi::c_void>,
											   pcb_data: Option<*mut ULONG>) -> windows::HRESULT {
		unsafe {
			HRESULT(core_bindings::IMetaDataImport2_GetCustomAttributeByName(metadata,
																			 tk_obj,
																			 sz_name.unwrap_or(0 as _),
																			 pp_data.unwrap_or(0 as _),
																			 pcb_data.unwrap_or(0 as _)) as u32)
		}
	}

	pub(crate) fn enum_interface_impls(metadata: *mut IMetaDataImport2, ph_enum: *mut HCORENUM,
									   td: Option<mdTypeDef>,
									   r_impls: Option<*mut mdInterfaceImpl>,
									   c_max: Option<ULONG>,
									   pc_impls: Option<*mut ULONG>) -> windows::HRESULT {
		unsafe {
			HRESULT(core_bindings::IMetaDataImport2_EnumInterfaceImpls(metadata, ph_enum,
																	   td.unwrap_or_default(),
																	   r_impls.unwrap_or(0 as _),
																	   c_max.unwrap_or(0 as _),
																	   pc_impls.unwrap_or(0 as _)) as u32)
		}
	}

	pub(crate) fn get_type_ref_props(metadata: *mut IMetaDataImport2, tr: mdTypeRef,
									 ptk_resolution_scope: Option<*mut mdToken>,
									 sz_name: Option<*mut u16>,
									 cch_name: Option<ULONG>,
									 pch_name: Option<*mut ULONG>) -> windows::HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IMetaDataImport2_GetTypeRefProps(metadata, tr,
																ptk_resolution_scope.unwrap_or(0 as _),
																sz_name.unwrap_or(0 as _),
																cch_name.unwrap_or_default(),
																pch_name.unwrap_or(0 as _)) as u32
			)
		}
	}

	pub(crate) fn find_method(metadata: *mut IMetaDataImport2,
							  td: mdTypeDef,
							  sz_name: LPCWSTR,
							  pv_sig_blob: Option<PCCOR_SIGNATURE>,
							  cb_sig_blob: ULONG,
							  pmb: *mut mdMethodDef) -> windows::HRESULT {
		unsafe {
			HRESULT(
				IMetaDataImport2_FindMethod(metadata, td, sz_name, pv_sig_blob.unwrap_or(0 as _), cb_sig_blob, pmb) as u32
			)
		}
	}

	pub(crate) fn enum_generic_params(
		metadata: *mut IMetaDataImport2,
		ph_enum: *mut HCORENUM,
		tk: mdToken,
		r_generic_params: Option<*mut mdGenericParam>,
		c_max: Option<ULONG>,
		pc_generic_params: Option<*mut ULONG>,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IMetaDataImport2_EnumGenericParams(
					metadata,
					ph_enum,
					tk,
					r_generic_params.unwrap_or(0 as _),
					c_max.unwrap_or_default(),
					pc_generic_params.unwrap_or(0 as _),
				) as u32
			)
		}
	}


	pub fn count_enum(
		metadata: *mut IMetaDataImport2,
		h_enum: HCORENUM,
		pul_count: *mut ULONG,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IMetaDataImport2_CountEnum(metadata, h_enum, pul_count) as u32
			)
		}
	}

	pub fn get_type_spec_from_token(
		metadata: *mut IMetaDataImport2,
		typespec: mdTypeSpec,
		ppv_sig: *mut PCCOR_SIGNATURE,
		pcb_sig: *mut ULONG,
	) -> HRESULT {
		unsafe {
			HRESULT(
				IMetaDataImport2_GetTypeSpecFromToken(
					metadata,
					typespec,
					ppv_sig,
					pcb_sig,
				) as u32
			)
		}
	}


	pub(crate) fn enum_fields(
		metadata: *mut IMetaDataImport2,
		ph_enum: *mut HCORENUM,
		tk_type_def: mdTypeDef,
		rg_fields: *mut mdFieldDef,
		c_max: ULONG,
		pc_tokens: *mut ULONG,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IMetaDataImport2_EnumFields(
					metadata,
					ph_enum,
					tk_type_def,
					rg_fields,
					c_max,
					pc_tokens,
				) as u32
			)
		}
	}

	pub(crate) fn enum_methods_with_name(
		metadata: *mut IMetaDataImport2,
		ph_enum: *mut HCORENUM,
		tk_type_def: mdTypeDef,
		sz_name: LPCWSTR,
		rg_methods: *mut mdMethodDef,
		c_max: ULONG,
		pc_tokens: *mut ULONG,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IMetaDataImport2_EnumMethodsWithName(
					metadata,
					ph_enum,
					tk_type_def,
					sz_name,
					rg_methods,
					c_max,
					pc_tokens,
				) as u32
			)
		}
	}

	pub(crate) fn get_interface_impl_props(
		metadata: *mut IMetaDataImport2,
		tk_interface_impl: mdInterfaceImpl,
		ptk_class: Option<*mut mdTypeDef>,
		ptk_iface: Option<*mut mdToken>,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IMetaDataImport2_GetInterfaceImplProps(
					metadata,
					tk_interface_impl,
					ptk_class.unwrap_or(0 as _),
					ptk_iface.unwrap_or(0 as _),
				) as u32
			)
		}
	}

	pub(crate) fn enum_methods(
		metadata: *mut IMetaDataImport2,
		ph_enum: *mut HCORENUM,
		tk_type_def: mdTypeDef,
		rg_methods: *mut mdMethodDef,
		c_max: ULONG,
		pc_tokens: *mut ULONG,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IMetaDataImport2_EnumMethods(
					metadata,
					ph_enum,
					tk_type_def,
					rg_methods,
					c_max,
					pc_tokens,
				) as u32
			)
		}
	}

	pub(crate) fn enum_properties(
		metadata: *mut IMetaDataImport2,
		ph_enum: *mut HCORENUM,
		tk_typ_def: mdTypeDef,
		rg_properties: *mut mdProperty,
		c_max: ULONG,
		pc_properties: *mut ULONG,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IMetaDataImport2_EnumProperties(
					metadata,
					ph_enum,
					tk_typ_def,
					rg_properties,
					c_max,
					pc_properties,
				) as u32
			)
		}
	}

	pub fn enum_events(
		metadata: *mut IMetaDataImport2,
		ph_enum: *mut HCORENUM,
		tk_typ_def: mdTypeDef,
		rg_events: *mut mdEvent,
		c_max: ULONG,
		pc_events: *mut ULONG,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IMetaDataImport2_EnumEvents(
					metadata,
					ph_enum,
					tk_typ_def,
					rg_events,
					c_max,
					pc_events,
				) as u32
			)
		}
	}

	pub(crate) fn get_event_props(
		metadata: *mut IMetaDataImport2,
		ev: mdEvent,
		p_class: Option<*mut mdTypeDef>,
		sz_event: Option<LPCWSTR>,
		cch_event: Option<ULONG>,
		pch_event: Option<*mut ULONG>,
		pdw_event_flags: Option<*mut DWORD>,
		ptk_event_type: Option<*mut mdToken>,
		pmd_add_on: Option<*mut mdMethodDef>,
		pmd_remove_on: Option<*mut mdMethodDef>,
		pmd_fire: Option<*mut mdMethodDef>,
		rmd_other_method: Option<*mut mdMethodDef>,
		c_max: Option<ULONG>,
		pc_other_method: Option<*mut ULONG>,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IMetaDataImport2_GetEventProps(
					metadata,
					ev,
					p_class.unwrap_or(0 as _),
					sz_event.unwrap_or(0 as _),
					cch_event.unwrap_or_default(),
					pch_event.unwrap_or(0 as _),
					pdw_event_flags.unwrap_or(0 as _),
					ptk_event_type.unwrap_or(0 as _),
					pmd_add_on.unwrap_or(0 as _),
					pmd_remove_on.unwrap_or(0 as _),
					pmd_fire.unwrap_or(0 as _),
					rmd_other_method.unwrap_or(0 as _),
					c_max.unwrap_or_default(),
					pc_other_method.unwrap_or(0 as _),
				) as u32
			)
		}
	}


	pub(crate) fn find_field(
		metadata: *mut IMetaDataImport2,
		td: mdTypeDef,
		sz_name: Option<LPCWSTR>,
		pv_sig_blob: Option<PCCOR_SIGNATURE>,
		cb_sig_blob: Option<ULONG>,
		pmb: Option<*mut mdFieldDef>,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IMetaDataImport2_FindField(
					metadata,
					td,
					sz_name.unwrap_or(0 as _),
					pv_sig_blob.unwrap_or(0 as _),
					cb_sig_blob.unwrap_or_default(),
					pmb.unwrap_or(0 as _),
				) as u32
			)
		}
	}

	pub(crate) fn get_member_ref_props(
		metadata: *mut IMetaDataImport2,
		tk_member_ref: mdMemberRef,
		ptk: Option<*mut mdToken>,
		sz_member: Option<LPWSTR>,
		cch_member: Option<ULONG>,
		pch_member: Option<*mut ULONG>,
		ppv_sig_blob: Option<*mut PCCOR_SIGNATURE>,
		pcb_sig_blob: Option<*mut ULONG>,
	) -> HRESULT {
		unsafe {
			HRESULT(core_bindings::IMetaDataImport2_GetMemberRefProps(
				metadata,
				tk_member_ref,
				ptk.unwrap_or(0 as _),
				sz_member.unwrap_or(0 as _),
				cch_member.unwrap_or_default(),
				pch_member.unwrap_or(0 as _),
				ppv_sig_blob.unwrap_or(0 as _),
				pcb_sig_blob.unwrap_or(0 as _),
			) as u32)
		}
	}

	pub fn get_custom_attribute_props(
		metadata: *mut IMetaDataImport2,
		cv: mdCustomAttribute,
		ptk_obj: Option<*mut mdToken>,
		ptk_type: Option<*mut mdToken>,
		pp_blob: Option<*mut *const BYTE>,
		pcb_blob: Option<*mut ULONG>,
	) -> HRESULT {
		HRESULT::from_win32(
			unsafe {
				core_bindings::IMetaDataImport2_GetCustomAttributeProps(
					metadata,
					cv,
					ptk_obj.unwrap_or(0 as _),
					ptk_type.unwrap_or(0 as _),
					pp_blob.unwrap_or(0 as _),
					pcb_blob.unwrap_or(0 as _),
				)
			} as u32
		)
	}

	pub fn enum_custom_attributes(
		metadata: *mut IMetaDataImport2,
		ph_enum: Option<*mut HCORENUM>,
		tk: Option<mdToken>,
		tk_type: Option<mdToken>,
		rg_custom_attributes: Option<*mut mdCustomAttribute>,
		c_max: Option<ULONG>,
		pc_custom_attributes: Option<*mut ULONG>,
	) -> HRESULT {
		unsafe {
			HRESULT::from_win32(
				core_bindings::IMetaDataImport2_EnumCustomAttributes(
					metadata,
					ph_enum.unwrap_or(0 as _),
					tk.unwrap_or_default(),
					tk_type.unwrap_or_default(),
					rg_custom_attributes.unwrap_or(0 as _),
					c_max.unwrap_or_default(),
					pc_custom_attributes.unwrap_or(0 as _),
				) as u32
			)
		}
	}

	pub(crate) fn find_type_def_by_name(
		metadata: *mut IMetaDataImport2,
		sz_type_def: Option<LPCWSTR>,
		tk_enclosing_class: Option<mdToken>,
		ptk_type_def: Option<*mut mdTypeDef>,
	) -> HRESULT {
		unsafe {
			HRESULT::from_win32(
				core_bindings::IMetaDataImport2_FindTypeDefByName(
					metadata,
					sz_type_def.unwrap_or(0 as _),
					tk_enclosing_class.unwrap_or(0 as _),
					ptk_type_def.unwrap_or(0 as _),
			) as u32
			)
		}
	}
}

pub mod iro_simple_meta_data_builder {
	use crate::prelude::*;
	use windows::{HRESULT, HSTRING};

	pub(crate) fn set_runtime_class_simple_default(
		builder: *mut IRoSimpleMetaDataBuilder,
		name: PCWSTR,
		default_interface_name: PCWSTR,
		default_interface_iid: *const GUID,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IRoSimpleMetaDataBuilder_SetRuntimeClassSimpleDefault(
					builder,
					name,
					default_interface_name,
					default_interface_iid,
				) as u32
			)
		}
	}


	pub(crate) fn set_win_rt_interface(
		builder: *mut IRoSimpleMetaDataBuilder,
		iid: GUID,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IRoSimpleMetaDataBuilder_SetWinRtInterface(
					builder,
					iid,
				) as u32
			)
		}
	}


	pub fn set_parameterized_interface(
		builder: *mut IRoSimpleMetaDataBuilder,
		piid: GUID,
		num_args: UINT32,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IRoSimpleMetaDataBuilder_SetParameterizedInterface(
					builder,
					piid,
					num_args,
				) as u32
			)
		}
	}


	pub fn set_enum(
		builder: *mut IRoSimpleMetaDataBuilder,
		name: PCWSTR,
		base_type: PCWSTR,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IRoSimpleMetaDataBuilder_SetEnum(
					builder,
					name,
					base_type,
				) as u32
			)
		}
	}


	pub fn set_struct(
		builder: *mut IRoSimpleMetaDataBuilder,
		name: PCWSTR,
		num_fields: UINT32,
		field_type_names: *const PCWSTR,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IRoSimpleMetaDataBuilder_SetStruct(
					builder,
					name,
					num_fields,
					field_type_names,
				) as u32
			)
		}
	}


	pub fn set_delegate(
		builder: *mut IRoSimpleMetaDataBuilder,
		iid: GUID,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IRoSimpleMetaDataBuilder_SetDelegate(
					builder,
					iid,
				) as u32
			)
		}
	}


	pub fn set_parameterized_delegate(
		builder: *mut IRoSimpleMetaDataBuilder,
		piid: GUID,
		num_args: UINT32,
	) -> HRESULT {
		unsafe {
			HRESULT(
				core_bindings::IRoSimpleMetaDataBuilder_SetParameterizedDelegate(
					builder,
					piid,
					num_args,
				) as u32
			)
		}
	}
}

pub mod enums {
	use crate::prelude::*;

	pub(crate) fn type_from_token(token: mdToken) -> ULONG {
		unsafe { core_bindings::Enums_TypeFromToken(token) }
	}
}

pub mod helpers {
	use crate::prelude::*;
	use core_bindings::PCCOR_SIGNATURE;
	use windows::HSTRING;

	pub(crate) fn get_type_name(metadata: *mut IMetaDataImport2, md_token: mdToken, name: *mut libc::wchar_t, size: ULONG) -> ULONG {
		unsafe { core_bindings::Helpers_Get_Type_Name(metadata, md_token, name, size) }
	}

	pub(crate) fn is_td_public(value: DWORD) -> bool {
		unsafe { core_bindings::Helpers_IsTdPublic(value) == 1 }
	}

	pub(crate) fn is_td_special_name(value: DWORD) -> bool {
		unsafe { core_bindings::Helpers_IsTdSpecialName(value) == 1 }
	}

	pub(crate) fn cor_sig_uncompress_calling_conv(p_data: PCCOR_SIGNATURE) -> ULONG {
		unsafe {
			core_bindings::Helpers_CorSigUncompressCallingConv(p_data)
		}
	}

	pub(crate) fn cor_sig_uncompress_data(p_data: PCCOR_SIGNATURE) -> ULONG {
		unsafe { core_bindings::Helpers_CorSigUncompressData(p_data) }
	}

	pub(crate) fn cor_sig_uncompress_element_type(p_data: PCCOR_SIGNATURE) -> crate::enums::CorElementType {
		unsafe {
			core_bindings::Helpers_CorSigUncompressElementType(p_data).into()
		}
	}

	pub(crate) fn cor_sig_uncompress_token(p_data: PCCOR_SIGNATURE) -> mdToken {
		unsafe {
			core_bindings::Helpers_CorSigUncompressToken(p_data)
		}
	}

	pub(crate) fn to_wstring(index: ULONG, text: *mut u16) -> usize {
		unsafe {
			core_bindings::Helpers_to_wstring(index, text)
		}
	}

	pub(crate) fn is_md_public(value: DWORD) -> bool {
		unsafe { core_bindings::Helpers_IsMdPublic(value) == 1 }
	}

	pub(crate) fn is_md_family(value: DWORD) -> bool {
		unsafe { core_bindings::Helpers_IsMdFamily(value) == 1 }
	}

	pub(crate) fn is_md_fam_orassem(value: DWORD) -> bool {
		unsafe { core_bindings::Helpers_IsMdFamORAssem(value) == 1 }
	}

	pub(crate) fn is_md_special_name(value: DWORD) -> bool {
		unsafe { core_bindings::Helpers_IsMdSpecialName(value) == 1 }
	}

	pub(crate) fn is_md_static(value: DWORD) -> bool {
		unsafe { core_bindings::Helpers_IsMdStatic(value) == 1 }
	}

	pub(crate) fn is_md_final(value: DWORD) -> bool {
		unsafe { core_bindings::Helpers_IsMdFinal(value) == 1 }
	}

	pub(crate) fn is_md_instance_initializer_w(flags: DWORD, name_data: *const u16) -> bool {
		unsafe {
			core_bindings::Helpers_IsMdInstanceInitializerW(flags, name_data) == 1
		}
	}

	pub(crate) fn is_pr_special_name(value: DWORD) -> bool {
		unsafe {
			core_bindings::Helpers_IsPrSpecialName(value) == 1
		}
	}

	pub(crate) fn bytes_to_guid(
		data: *mut u8,
		guid: *mut GUID,
	) {
		unsafe {
			core_bindings::Helpers_bytesToGuid(data, guid)
		}
	}

	pub(crate) fn windows_get_string_raw_buffer(mut string: HSTRING, length: Option<*mut UINT32>) -> PCWSTR {
		unsafe {
			core_bindings::Helpers_WindowsGetStringRawBuffer(std::mem::transmute(&mut string), length.unwrap_or(0 as _))
		}
	}

	pub(crate) fn is_td_interface(value: DWORD) -> bool {
		unsafe {
			core_bindings::Helpers_IsTdInterface(value) == 1
		}
	}

	pub(crate) fn is_td_class(value: DWORD) -> bool {
		unsafe {
			core_bindings::Helpers_IsTdClass(value) == 1
		}
	}

	pub fn is_ev_special_name(value: DWORD) -> bool {
		unsafe {
			core_bindings::Helpers_IsEvSpecialName(value) == 1
		}
	}

	pub(crate) fn generate_id_name(
		name_parts_w: *mut PCWSTR,
		declaration_full_name: *mut u16,
		name_parts_count: *mut DWORD,
	) {
		unsafe {
			core_bindings::Helpers_generate_id_name(
				name_parts_w,
				declaration_full_name,
				name_parts_count,
			)
		}
	}

	pub(crate) fn to_string_length(value: PCWSTR, count: *mut usize) {
		unsafe {
			core_bindings::Helpers_toString_length(value, count);
		}
	}


	pub(crate) fn is_td_sealed(value: DWORD) -> bool {
		unsafe { core_bindings::Helpers_IsTdSealed(value) == 1 }
	}
}



