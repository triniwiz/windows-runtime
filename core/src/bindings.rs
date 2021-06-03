use prelude::*;

extern {
    fn Rometadataresolution_RoGetMetaDataFile(
        name: windows::HSTRING, meta_data_dispenser: *const c_void, meta_data_file_path: *const c_void, meta_data_import: *mut *mut c_void, type_def_token: *mut u32,
    ) -> c_long;

    fn IMetaDataImport2_GetTypeDefProps(metadata: *const c_void, md_type_def: c_uint, sz_type_def: *mut c_void, cch_type_def: i64, pch_type_def: *mut i64, pdw_type_def_flags: *mut i64, md_token: *mut u32) -> windows::HRESULT;

    fn IMetaDataImport2_GetTypeDefPropsNameSize(metadata: *const c_void, md_type_def: c_uint, pch_type_def: *mut i64);

    fn IMetaDataImport2_EnumInterfaceImpls(metadata: *const c_void, md_type_def: c_uint);

    fn Enums_TypeFromToken(token: mdToken) -> ULONG;

    fn Helpers_Get_Type_Name(meta: *const c_void, md_token: mdToken, name: *mut libc::wchar_t, size: usize) -> ULONG;

    fn Helpers_IsTdPublic(value: DWORD) -> bool;

    fn Helpers_IsTdSpecialName(value: DWORD) -> bool;
}


pub mod rometadataresolution {
    fn ro_get_meta_data_file(
        name: windows::HSTRING, meta_data_dispenser: *const c_void, meta_data_file_path: *const c_void, meta_data_import: *mut *mut c_void, type_def_token: *mut u32,
    ) -> c_long {
        Rometadataresolution_RoGetMetaDataFile(name, meta_data_dispenser, meta_data_file_path, meta_data_import, type_def_token)
    }
}

pub mod imeta_data_import2 {
    fn get_type_def_props(metadata: *const c_void, md_type_def: c_uint, sz_type_def: *mut c_void, cch_type_def: i64, pch_type_def: *mut i64, pdw_type_def_flags: *mut i64, md_token: *mut u32) {
        IMetaDataImport2_GetTypeDefProps(metadata, md_type_def, sz_type_def, cch_type_def, pch_type_def, pdw_type_def_flags, md_token)
    }

    fn get_type_def_props_name_size(metadata: *const c_void, md_type_def: c_uint, pch_type_def: *mut i64) {
        IMetaDataImport2_GetTypeDefPropsNameSize(metadata, md_type_def, pch_type_def)
    }
}

pub mod enums {
    fn type_from_token(token: mdToken) -> ULONG {
        Enums_TypeFromToken(token)
    }
}

pub mod helpers {
    fn get_type_name(meta: *const c_void, md_token: mdToken, name: *mut libc::wchar_t, size: usize) -> ULONG {
        Helpers_Get_Type_Name(meta, md_token, name, size)
    }

    fn is_td_public(value: DWORD) -> bool {
        Helpers_IsTdPublic(value)
    }

    fn is_td_special_name(value: DWORD) -> bool {
        Helpers_IsTdSpecialName(value)
    }
}