use crate::prelude::*;
use windows::HRESULT;
pub struct IMetaDataImport2(IMetaDataImport2_);


impl IMetaDataImport2 {
    pub(crate) fn empty() -> Self {
        Self(std::ptr::null_mut())
    }
    pub(crate) fn inner(&self) -> &core_bindings::IMetaDataImport2 {
        &self.0
    }
    pub fn get_type_def_props(
        &self,
        md_type_def: c_uint,
        sz_type_def: Option<&mut [u16]>,
        cch_type_def: Option<ULONG>,
        pch_type_def: Option<&mut ULONG>,
        pdw_type_def_flags: Option<&mut ULONG>,
        md_token: Option<&mut ULONG32>,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_GetTypeDefProps(
                &self.0 as _,
                md_type_def,
                sz_type_def.unwrap_or_default().as_mut_ptr(),
                cch_type_def.unwrap_or(0),
                pch_type_def.unwrap_or(&mut 0),
                pdw_type_def_flags.unwrap_or(0 as _),
                md_token.unwrap_or(&mut 0),
            ) as u32)
        }
    }

    pub fn get_type_def_props_name_size(
        &self,
        md_type_def: c_uint,
        pch_type_def: &mut ULONG,
    ) -> HRESULT {
        unsafe {
            HRESULT(core_bindings::IMetaDataImport2_GetTypeDefPropsNameSize(
                &self.0 as _,
                md_type_def,
                pch_type_def,
            ) as u32)
        }
    }

    pub fn get_field_props(
        &self,
        mb: mdFieldDef,
        p_class: Option<&mut mdTypeDef>,
        sz_field: Option<&mut [u16]>,
        cch_field: Option<ULONG>,
        pch_field: Option<&mut ULONG>,
        pdw_attr: Option<&mut DWORD>,
        ppv_sig_blob: Option<*mut PCCOR_SIGNATURE>,
        pcb_sig_blob: Option<&mut ULONG>,
        pdw_cplus_type_flag: Option<&mut DWORD>,
        pp_value: Option<&mut UVCP_CONSTANT>,
        pcch_value: Option<&mut ULONG>,
    ) -> HRESULT {
        unsafe {
            HRESULT(core_bindings::IMetaDataImport2_GetFieldProps(
                &self.0 as _,
                mb,
                p_class.unwrap_or(0 as _),
                sz_field.unwrap_or_default().as_mut_ptr(),
                cch_field.unwrap_or_default(),
                pch_field.unwrap_or(0 as _),
                pdw_attr.unwrap_or(0 as _),
                ppv_sig_blob.unwrap_or(0 as _),
                pcb_sig_blob.unwrap_or(0 as _),
                pdw_cplus_type_flag.unwrap_or(0 as _),
                pp_value.unwrap_or(0 as _),
                pcch_value.unwrap_or(0 as _),
            ) as u32)
        }
    }

    pub fn get_property_props(
        &self,
        prop: mdProperty,
        p_class: Option<&mut mdTypeDef>,
        sz_property: Option<&mut [u16]>,
        cch_property: Option<ULONG>,
        pch_property: Option<&mut ULONG>,
        pdw_prop_flags: Option<*mut DWORD>,
        ppv_sig: Option<*mut PCCOR_SIGNATURE>,
        pb_sig: Option<&mut ULONG>,
        pdw_cplus_type_flag: Option<&mut DWORD>,
        pp_default_value: Option<*mut UVCP_CONSTANT>,
        pcch_default_value: Option<*mut ULONG>,
        pmd_setter: Option<&mut mdMethodDef>,
        pmd_getter: Option<&mut mdMethodDef>,
        rmd_other_method: Option<&mut [mdMethodDef]>,
        c_max: Option<ULONG>,
        pc_other_method: Option<*mut ULONG>,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_GetPropertyProps(
                &self.0 as _,
                prop,
                p_class.unwrap_or(0 as _),
                sz_property.unwrap_or_default().as_mut_ptr(),
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
                rmd_other_method.unwrap_or_default().as_mut_ptr(),
                c_max.unwrap_or_default(),
                pc_other_method.unwrap_or(0 as _),
            ) as u32)
        }
    }

    pub fn get_method_props(
        &self,
        tk_method_def: mdMethodDef,
        ptk_class: Option<&mut mdTypeDef>,
        sz_method: Option<&mut [u16]>,
        cch_method: Option<ULONG>,
        pch_method: Option<&mut ULONG>,
        pdw_attr: Option<&mut DWORD>,
        ppv_sig_blob: Option<*mut PCCOR_SIGNATURE>,
        pcb_sig_blob: Option<&mut ULONG>,
        pul_code_rva: Option<&mut ULONG>,
        pdw_impl_flags: Option<&mut DWORD>,
    ) -> windows::HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_GetMethodProps(
                &self.0 as _,
                tk_method_def,
                ptk_class.unwrap_or(0 as _),
                sz_method.unwrap_or_default().as_mut_ptr(),
                cch_method.unwrap_or_default(),
                pch_method.unwrap_or(0 as _),
                pdw_attr.unwrap_or(0 as _),
                ppv_sig_blob.unwrap_or(0 as _),
                pcb_sig_blob.unwrap_or(0 as _),
                pul_code_rva.unwrap_or(0 as _),
                pdw_impl_flags.unwrap_or(0 as _),
            ) as u32)
        }
    }

    pub fn enum_params(
        &self,
        ph_enum: *mut HCORENUM,
        mb: mdMethodDef,
        r_params: &mut [mdParamDef],
        c_max: ULONG,
        pc_tokens: &mut ULONG,
    ) -> windows::HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_EnumParams(
                &self.0 as _,
                ph_enum,
                mb,
                r_params.as_mut_ptr(),
                c_max,
                pc_tokens,
            ) as u32)
        }
    }

    pub fn close_enum(&self, ph_enum: HCORENUM) {
        unsafe { core_bindings::IMetaDataImport2_CloseEnum(&self.0 as _, ph_enum) }
    }

    pub fn get_param_props(
        &self,
        tk: mdParamDef,
        pmd: Option<&mut mdMethodDef>,
        pul_sequence: Option<&mut ULONG>,
        sz_name: Option<&mut [u16]>,
        cch_name: Option<ULONG>,
        pch_name: Option<&mut ULONG>,
        pdw_attr: Option<&mut DWORD>,
        pdw_cplus_type_flag: Option<&mut DWORD>,
        pp_value: Option<*mut UVCP_CONSTANT>,
        pcch_value: Option<&mut ULONG>,
    ) -> windows::HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_GetParamProps(
                self.0 as _,
                tk,
                pmd.unwrap_or(0 as _),
                pul_sequence.unwrap_or(0 as _),
                sz_name.unwrap_or_default().as_mut_ptr(),
                cch_name.unwrap_or_default(),
                pch_name.unwrap_or(0 as _),
                pdw_attr.unwrap_or(0 as _),
                pdw_cplus_type_flag.unwrap_or(0 as _),
                pp_value.unwrap_or(0 as _),
                pcch_value.unwrap_or(0 as _),
            ) as u32)
        }
    }

    pub fn get_custom_attribute_by_name(
        &self,
        tk_obj: mdToken,
        sz_name: Option<&[u16]>,
        pp_data: Option<*mut *const ::core::ffi::c_void>,
        pcb_data: Option<&mut ULONG>,
    ) -> windows::HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_GetCustomAttributeByName(
                &self.0 as _,
                tk_obj,
                sz_name.unwrap_or_default().as_ptr(),
                pp_data.unwrap_or(0 as _),
                pcb_data.unwrap_or(0 as _),
            ) as u32)
        }
    }

    pub fn enum_interface_impls(
        &self,
        ph_enum: *mut HCORENUM,
        td: Option<mdTypeDef>,
        r_impls: Option<&mut [mdInterfaceImpl]>,
        c_max: Option<ULONG>,
        pc_impls: Option<&mut ULONG>,
    ) -> windows::HRESULT {
        unsafe {
            HRESULT(core_bindings::IMetaDataImport2_EnumInterfaceImpls(
                &self.0 as _,
                ph_enum,
                td.unwrap_or_default(),
                r_impls.unwrap_or_default().as_mut_ptr(),
                c_max.unwrap_or(0 as _),
                pc_impls.unwrap_or(0 as _),
            ) as u32)
        }
    }

    pub fn get_type_ref_props(
        &self,
        tr: mdTypeRef,
        ptk_resolution_scope: Option<&mut mdToken>,
        sz_name: Option<&mut [u16]>,
        cch_name: Option<ULONG>,
        pch_name: Option<&mut ULONG>,
    ) -> windows::HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_GetTypeRefProps(
                &self.0 as _,
                tr,
                ptk_resolution_scope.unwrap_or(0 as _),
                sz_name.unwrap_or_default().as_mut_ptr(),
                cch_name.unwrap_or_default(),
                pch_name.unwrap_or(0 as _),
            ) as u32)
        }
    }

    pub fn find_method(
        &self,
        td: mdTypeDef,
        sz_name: LPCWSTR,
        pv_sig_blob: Option<PCCOR_SIGNATURE>,
        cb_sig_blob: Option<ULONG>,
        pmb: Option<&mut mdMethodDef>,
    ) -> windows::HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_FindMethod(
                &self.0 as _,
                td,
                sz_name,
                pv_sig_blob.unwrap_or(0 as _),
                cb_sig_blob.unwrap_or_default(),
                pmb.unwrap_or(0 as _),
            ) as u32)
        }
    }

    pub fn enum_generic_params(
        &self,
        ph_enum: *mut HCORENUM,
        tk: mdToken,
        r_generic_params: Option<&mut [mdGenericParam]>,
        c_max: Option<ULONG>,
        pc_generic_params: Option<&mut ULONG>,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_EnumGenericParams(
                &self.0 as _,
                ph_enum,
                tk,
                r_generic_params.unwrap_or_default().as_mut_ptr(),
                c_max.unwrap_or_default(),
                pc_generic_params.unwrap_or(0 as _),
            ) as u32)
        }
    }

    pub fn count_enum(&self, h_enum: HCORENUM, pul_count: *mut ULONG) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_CountEnum(
                &self.0 as _,
                h_enum,
                pul_count,
            ) as u32)
        }
    }

    pub fn get_type_spec_from_token(
        &self,
        typespec: mdTypeSpec,
        ppv_sig: *mut PCCOR_SIGNATURE,
        pcb_sig: &mut ULONG,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_GetTypeSpecFromToken(
                &self.0 as _,
                typespec,
                ppv_sig,
                pcb_sig,
            ) as u32)
        }
    }

    pub fn enum_fields(
        &self,
        ph_enum: *mut HCORENUM,
        tk_type_def: mdTypeDef,
        rg_fields: &mut [mdFieldDef],
        c_max: ULONG,
        pc_tokens: &mut ULONG,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_EnumFields(
                &self.0 as _,
                ph_enum,
                tk_type_def,
                rg_fields,
                c_max,
                pc_tokens,
            ) as u32)
        }
    }

    pub fn enum_methods_with_name(
        &self,
        ph_enum: &mut HCORENUM,
        tk_type_def: mdTypeDef,
        sz_name: LPCWSTR,
        rg_methods: &mut [mdMethodDef],
        c_max: ULONG,
        pc_tokens: &mut ULONG,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_EnumMethodsWithName(
                &self.0 as _,
                ph_enum,
                tk_type_def,
                sz_name,
                rg_methods.as_mut_ptr(),
                c_max,
                pc_tokens,
            ) as u32)
        }
    }

    pub fn get_interface_impl_props(
        &self,
        tk_interface_impl: mdInterfaceImpl,
        ptk_class: Option<&mut mdTypeDef>,
        ptk_iface: Option<&mut mdToken>,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_GetInterfaceImplProps(
                &self.0 as _,
                tk_interface_impl,
                ptk_class.unwrap_or(0 as _),
                ptk_iface.unwrap_or(0 as _),
            ) as u32)
        }
    }

    pub fn enum_methods(
        &self,
        ph_enum: &mut HCORENUM,
        tk_type_def: mdTypeDef,
        rg_methods: &mut [mdMethodDef],
        c_max: ULONG,
        pc_tokens: &mut ULONG,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_EnumMethods(
                &self.0 as _,
                ph_enum,
                tk_type_def,
                rg_methods.as_mut_ptr(),
                c_max,
                pc_tokens,
            ) as u32)
        }
    }

    pub fn enum_properties(
        &self,
        ph_enum: *mut HCORENUM,
        tk_typ_def: mdTypeDef,
        rg_properties: &mut [mdProperty],
        c_max: ULONG,
        pc_properties: &mut ULONG,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_EnumProperties(
                &self.0 as _,
                ph_enum,
                tk_typ_def,
                rg_properties,
                c_max,
                pc_properties,
            ) as u32)
        }
    }

    pub fn enum_events(
        &self,
        ph_enum: *mut HCORENUM,
        tk_typ_def: mdTypeDef,
        rg_events: &mut [mdEvent],
        c_max: ULONG,
        pc_events: &mut ULONG,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_EnumEvents(
                &self.0 as _,
                ph_enum,
                tk_typ_def,
                rg_events,
                c_max,
                pc_events,
            ) as u32)
        }
    }

    pub fn get_event_props(
        &self,
        ev: mdEvent,
        p_class: Option<&mut mdTypeDef>,
        sz_event: Option<LPCWSTR>,
        cch_event: Option<ULONG>,
        pch_event: Option<&mut ULONG>,
        pdw_event_flags: Option<&mut DWORD>,
        ptk_event_type: Option<&mut mdToken>,
        pmd_add_on: Option<&mut mdMethodDef>,
        pmd_remove_on: Option<&mut mdMethodDef>,
        pmd_fire: Option<&mut mdMethodDef>,
        rmd_other_method: Option<&mut [mdMethodDef]>,
        c_max: Option<ULONG>,
        pc_other_method: Option<&mut ULONG>,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_GetEventProps(
                &self.0 as _,
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
                rmd_other_method.unwrap_or_default().as_mut_ptr(),
                c_max.unwrap_or_default(),
                pc_other_method.unwrap_or(0 as _),
            ) as u32)
        }
    }

    pub fn find_field(
        &self,
        td: mdTypeDef,
        sz_name: Option<LPCWSTR>,
        pv_sig_blob: Option<PCCOR_SIGNATURE>,
        cb_sig_blob: Option<ULONG>,
        pmb: Option<&mut mdFieldDef>,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_FindField(
                &self.0 as _,
                td,
                sz_name.unwrap_or(0 as _),
                pv_sig_blob.unwrap_or(0 as _),
                cb_sig_blob.unwrap_or_default(),
                pmb.unwrap_or(0 as _),
            ) as u32)
        }
    }

    pub fn get_member_ref_props(
        &self,
        tk_member_ref: mdMemberRef,
        ptk: Option<&mut mdToken>,
        sz_member: Option<LPWSTR>,
        cch_member: Option<ULONG>,
        pch_member: Option<&mut ULONG>,
        ppv_sig_blob: Option<*mut PCCOR_SIGNATURE>,
        pcb_sig_blob: Option<&mut ULONG>,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_GetMemberRefProps(
                &self.0 as _,
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
        &self,
        cv: mdCustomAttribute,
        ptk_obj: Option<&mut mdToken>,
        ptk_type: Option<&mut mdToken>,
        pp_blob: Option<*mut *const BYTE>,
        pcb_blob: Option<&mut ULONG>,
    ) -> HRESULT {
        HRESULT::from_win32(unsafe {
            core_bindings::IMetaDataImport2_GetCustomAttributeProps(
                &self.0 as _,
                cv,
                ptk_obj.unwrap_or(0 as _),
                ptk_type.unwrap_or(0 as _),
                pp_blob.unwrap_or(0 as _),
                pcb_blob.unwrap_or(0 as _),
            )
        } as u32)
    }

    pub fn enum_custom_attributes(
        &self,
        ph_enum: Option<*mut HCORENUM>,
        tk: Option<mdToken>,
        tk_type: Option<mdToken>,
        rg_custom_attributes: Option<&mut [mdCustomAttribute]>,
        c_max: Option<ULONG>,
        pc_custom_attributes: Option<*mut ULONG>,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_EnumCustomAttributes(
                &self.0 as _,
                ph_enum.unwrap_or(0 as _),
                tk.unwrap_or_default(),
                tk_type.unwrap_or_default(),
                rg_custom_attributes.unwrap_or(0 as _),
                c_max.unwrap_or_default(),
                pc_custom_attributes.unwrap_or(0 as _),
            ) as u32)
        }
    }

    pub fn find_type_def_by_name(
        &self,
        sz_type_def: Option<LPCWSTR>,
        tk_enclosing_class: Option<mdToken>,
        ptk_type_def: Option<&mut mdTypeDef>,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_FindTypeDefByName(
                &self.0 as _,
                sz_type_def.unwrap_or(0 as _),
                tk_enclosing_class.unwrap_or(0 as _),
                ptk_type_def.unwrap_or(0 as _),
            ) as u32)
        }
    }

    pub fn enum_method_impls(
        &self,
        ph_enum: *mut HCORENUM,
        tk_type_def: Option<mdTypeDef>,
        r_method_body: Option<&mut [mdToken]>,
        r_method_decl: Option<&mut [mdToken]>,
        c_max: Option<ULONG>,
        pc_tokens: Option<&mut ULONG>,
    ) -> HRESULT {
        unsafe {
            HRESULT::from_win32(core_bindings::IMetaDataImport2_EnumMethodImpls(
                &self.0 as _,
                ph_enum,
                tk_type_def.unwrap_or_default(),
                r_method_body.unwrap_or_default().as_mut_ptr(),
                r_method_decl.unwrap_or_default().as_mut_ptr(),
                c_max.unwrap_or_default(),
                pc_tokens.unwrap_or(0 as _),
            ) as u32)
        }
    }
}
