use std::ffi::c_void;
use std::mem;
use std::mem::MaybeUninit;
use std::ptr::{addr_of, addr_of_mut};
use std::slice::Windows;
use std::sync::Arc;
use parking_lot::RwLock;
use windows::core::{ComInterface, GUID, HRESULT, HSTRING, IInspectable, Interface, IUnknown, Type};
use metadata::declarations::declaration::{Declaration, DeclarationKind};
use libffi::low::*;
use libffi::raw::ffi_call;
use v8::FunctionCallbackArguments;
use windows::Win32::System::WinRT::IActivationFactory;
use metadata::declarations::base_class_declaration::BaseClassDeclarationImpl;
use metadata::declarations::delegate_declaration::generic_delegate_declaration::GenericDelegateDeclaration;
use metadata::declarations::interface_declaration::generic_interface_declaration::GenericInterfaceDeclaration;
use metadata::declarations::interface_declaration::generic_interface_instance_declaration::GenericInterfaceInstanceDeclaration;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::declarations::method_declaration::MethodDeclaration;
use metadata::declarations::parameter_declaration::ParameterDeclaration;
use metadata::declarations::property_declaration::PropertyDeclaration;
use metadata::declaring_interface_for_method::Metadata;
use metadata::signature::Signature;
use crate::DeclarationFFI;

pub enum Value {
    Constructor(Arc<RwLock<dyn Declaration>>),
    Object(Arc<RwLock<dyn Declaration>>),
}

pub struct MethodCall {
    index: usize,
    number_of_parameters: usize,
    number_of_abi_parameters: usize,
    is_initializer: bool,
    is_sealed: bool,
    is_void: bool,
    iid: GUID,
    cif: ffi_cif,
    interface: IUnknown,
    parameter_types: Vec<*mut ffi_type>,
    parameters: Vec<ParameterDeclaration>,
    return_type: String,
    pub(crate) declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>>,
}

impl MethodCall {

    pub fn is_void(&self) -> bool {
        self.is_void
    }

    pub fn return_type(&self) -> &str {
        self.return_type.as_str()
    }

    pub fn new(method: &MethodDeclaration, is_sealed: bool, interface: IUnknown, is_initializer: bool) -> MethodCall {
        let number_of_parameters = method.number_of_parameters();

        let mut index = 0 as usize;

        let mut declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>> = None;

        let iid = match Metadata::find_declaring_interface_for_method(method, &mut index) {
            None => {
                index = 0;
                IActivationFactory::IID
            }
            Some(interface) => {
                let iid;
                {
                    let ii_lock = interface.read();

                    let kind = ii_lock.base().kind();

                    match kind {
                        DeclarationKind::GenericInterfaceInstance => {
                            let ii = ii_lock.as_declaration().as_any().downcast_ref::<GenericInterfaceInstanceDeclaration>();
                            let ii = ii.unwrap();
                            iid = ii.id();
                        }
                        _ => {
                            let ii = ii_lock.as_declaration().as_any().downcast_ref::<InterfaceDeclaration>();
                            let ii = ii.unwrap();
                            iid = ii.id();
                        }
                    }
                }
                declaration = Some(interface);
                iid
            }
        };

        index = index.saturating_add(6); // account for IInspectable vtable overhead

        let mut interface_ptr: *mut c_void = std::ptr::null_mut(); // IActivationFactory

        let vtable = interface.vtable();

        let interface_ptr_ptr = addr_of_mut!(interface_ptr);

        let result = unsafe { ((*vtable).QueryInterface)(interface.as_raw(), &iid, interface_ptr_ptr as *mut *const c_void) };

        assert!(result.is_ok());

        let is_composition = !is_sealed;

        let is_void = method.is_void();

        let other_params: usize = if is_initializer {
            if is_sealed { 2 } else { 4 }
        } else {
            if is_void { 1 } else { 2 }
        };

        let number_of_abi_parameters = number_of_parameters + other_params;

        let mut parameter_types: Vec<*mut ffi_type> = Vec::new();

        parameter_types.reserve(number_of_abi_parameters);

        unsafe { parameter_types.push(&mut types::pointer); }

        for parameter in method.parameters().iter() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata().unwrap();

            let signature = Signature::to_string(metadata, &type_);

            match signature.as_str() {
                "String" => {
                    unsafe { parameter_types.push(&mut types::pointer); }
                }
                "Boolean" => {
                    unsafe { parameter_types.push(&mut types::uint8); }
                }
                _ => {
                    // objects
                    unsafe { parameter_types.push(&mut types::pointer); }
                }
            }
        }

        if is_initializer {
            if is_composition {
                unsafe { parameter_types.push(&mut types::pointer); }
                unsafe { parameter_types.push(&mut types::pointer); }
            }

            unsafe { parameter_types.push(&mut types::pointer); }
        } else {
            if !is_void {
                unsafe { parameter_types.push(&mut types::pointer); }
            }
        }

        let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };

        let mut cif: ffi_cif = Default::default();

        let prep_result = unsafe {
            prep_cif(&mut cif,
                     ffi_abi_FFI_DEFAULT_ABI,
                     parameter_types.len(),
                     &mut types::sint32,
                     parameter_types.as_mut_ptr(),
            )
        };


        assert!(prep_result.is_ok());

        // todo handle prep_cif error

        let signature = method.return_type();
        let return_type = Signature::to_string(method.metadata().unwrap(), &signature);
        Self {
            index,
            number_of_parameters,
            number_of_abi_parameters,
            is_initializer,
            is_sealed,
            is_void: method.is_void(),
            iid,
            cif,
            interface,
            parameter_types,
            parameters: method.parameters().to_vec(),
            declaration,
            return_type,
        }
    }

    pub fn call(&mut self, scope: &mut v8::HandleScope, args: &FunctionCallbackArguments) -> (HRESULT, *mut c_void) {
        let number_of_abi_parameters = self.number_of_abi_parameters;

        let mut arguments: Vec<*mut c_void> = Vec::new();

        arguments.reserve(number_of_abi_parameters);

        unsafe { arguments.push(self.interface.as_raw()) };

        let mut string_buf: Vec<HSTRING> = Vec::new();

        for (i, parameter) in self.parameters.iter().enumerate() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata().unwrap();

            let signature = Signature::to_string(metadata, &type_);

            println!("sig {}", signature.as_str());

            match signature.as_str() {
                "String" => {
                    let string = args.get(i as i32).to_string(scope).unwrap();

                    let string = HSTRING::from(string.to_rust_string_lossy(scope));;

                    arguments.push(
                        addr_of!(string) as *mut c_void
                    );

                   string_buf.push(string);
                }
                "Boolean" => {
                    let value = args.get(i as i32).boolean_value(scope);
                    // arguments.push(
                    //     addr_of!(value) as *mut c_void
                    // )

                    arguments.push(
                        unsafe { std::mem::transmute(&value) }
                    )
                }
                _ => {
                    let value = args.get(i as i32);

                    if value.is_object() {
                        let value = value.to_object(scope).unwrap();

                        let dec = value.get_internal_field(scope, 0).unwrap();

                        let dec = unsafe { v8::Local::<v8::External>::cast(dec) };

                        let dec = dec.value() as *mut DeclarationFFI;

                        let dec = unsafe { &*dec };

                        let instance = dec.instance.clone();
                        match instance {
                            None => {
                                arguments.push(std::ptr::null_mut());
                            }
                            Some(mut instance) => {
                                unsafe { arguments.push(&mut instance.into_raw() as *mut _ as *mut c_void) };
                            }
                        }
                    }
                }
            }
        }

        let mut result: *mut c_void = std::ptr::null_mut();

        let mut result_ptr: *mut *mut *mut c_void = &mut addr_of_mut!(result);

        if self.is_initializer {
            arguments.push(result_ptr as *mut c_void);
        } else {
            if !self.is_void {
                match self.return_type.as_str() {
                    "Boolean" | "String" => {
                       // arguments.push(&mut result as *mut _ as *mut c_void);
                        arguments.push(addr_of_mut!(result) as *mut _ as *mut c_void);
                    }
                    _ => {
                        arguments.push(result_ptr as *mut c_void);
                    }
                }
            }
        }

        let mut vtable = self.interface.vtable();

        let mut vtable: *mut *mut c_void = unsafe { std::mem::transmute(vtable) };

        let func = unsafe {
            *vtable.offset(self.index as isize)
        };

        if !self.is_initializer {
            let raw = self.interface.as_raw();
            let json = unsafe { windows::Data::Json::JsonObject::from_raw_borrowed(&raw)};
            let json = json.unwrap();
            println!("aasdasd {}", json.Size().unwrap());
        }

        let ret = unsafe {
            call::<i32>(&mut self.cif, CodePtr::from_ptr(func), arguments.as_mut_ptr())
        };

        (HRESULT(ret), result)
    }
}



pub struct PropertyCall {
    index: usize,
    number_of_parameters: usize,
    number_of_abi_parameters: usize,
    is_initializer: bool,
    is_sealed: bool,
    is_void: bool,
    is_setter: bool,
    iid: GUID,
    cif: ffi_cif,
    interface: IUnknown,
    parameter_types: Vec<*mut ffi_type>,
    parameters: Vec<ParameterDeclaration>,
    return_type: String,
    pub(crate) declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>>,
}

impl PropertyCall {

    pub fn is_void(&self) -> bool {
        self.is_void
    }

    pub fn return_type(&self) -> &str {
        self.return_type.as_str()
    }

    pub fn new(property: &PropertyDeclaration, is_setter: bool, interface: IUnknown, is_initializer: bool) -> Self {

        let method = if is_setter {
            property.setter().unwrap()
        }else {
            property.getter()
        };

        let number_of_parameters = method.number_of_parameters();

        let mut index = 0 as usize;

        let mut declaration: Option<Arc<RwLock<dyn BaseClassDeclarationImpl>>> = None;

        let iid = match Metadata::find_declaring_interface_for_method(method, &mut index) {
            None => {
                index = 0;
                IActivationFactory::IID
            }
            Some(interface) => {
                let iid;
                {
                    let ii_lock = interface.read();

                    let kind = ii_lock.base().kind();

                    match kind {
                        DeclarationKind::GenericInterfaceInstance => {
                            let ii = ii_lock.as_declaration().as_any().downcast_ref::<GenericInterfaceInstanceDeclaration>();
                            let ii = ii.unwrap();
                            iid = ii.id();
                        }
                        _ => {
                            let ii = ii_lock.as_declaration().as_any().downcast_ref::<InterfaceDeclaration>();
                            let ii = ii.unwrap();
                            iid = ii.id();
                        }
                    }
                }
                declaration = Some(interface);
                iid
            }
        };

        index = index.saturating_add(6); // account for IInspectable vtable overhead

        let mut interface_ptr: *mut c_void = std::ptr::null_mut(); // IActivationFactory

        let vtable = interface.vtable();

        let interface_ptr_ptr = addr_of_mut!(interface_ptr);

        let result = unsafe { ((*vtable).QueryInterface)(interface.as_raw(), &iid, interface_ptr_ptr as *mut *const c_void) };

        assert!(result.is_ok());

        let is_sealed = method.is_sealed();

        let is_composition = !is_sealed;

        let is_void = method.is_void();

        let other_params: usize = if is_initializer {
            if is_sealed { 2 } else { 4 }
        } else {
            if is_void { 1 } else { 2 }
        };

        let number_of_abi_parameters = number_of_parameters + other_params;

        let mut parameter_types: Vec<*mut ffi_type> = Vec::new();

        parameter_types.reserve(number_of_abi_parameters);

        unsafe { parameter_types.push(&mut types::pointer); }

        for parameter in method.parameters().iter() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata().unwrap();

            let signature = Signature::to_string(metadata, &type_);

            match signature.as_str() {
                "String" => {
                    unsafe { parameter_types.push(&mut types::pointer); }
                }
                "Boolean" => {
                    unsafe { parameter_types.push(&mut types::uint8); }
                }
                _ => {
                    // objects
                    unsafe { parameter_types.push(&mut types::pointer); }
                }
            }
        }

        if is_initializer {
            if is_composition {
                unsafe { parameter_types.push(&mut types::pointer); }
                unsafe { parameter_types.push(&mut types::pointer); }
            }

            unsafe { parameter_types.push(&mut types::pointer); }
        } else {
            if !is_void {
                unsafe { parameter_types.push(&mut types::pointer); }
            }
        }

        let interface = unsafe { IUnknown::from_raw(interface_ptr as *mut c_void) };

        let mut cif: ffi_cif = Default::default();

        let prep_result = unsafe {
            prep_cif(&mut cif,
                     ffi_abi_FFI_DEFAULT_ABI,
                     parameter_types.len(),
                     &mut types::sint32,
                     parameter_types.as_mut_ptr(),
            )
        };


        assert!(prep_result.is_ok());

        // todo handle prep_cif error

        let signature = method.return_type();
        let return_type = Signature::to_string(method.metadata().unwrap(), &signature);

        Self {
            index,
            number_of_parameters,
            number_of_abi_parameters,
            is_initializer,
            is_sealed,
            is_void: method.is_void(),
            iid,
            cif,
            interface,
            parameter_types,
            parameters: method.parameters().to_vec(),
            declaration,
            return_type,
            is_setter
        }
    }

    pub fn call(&mut self, scope: &mut v8::HandleScope, args: &v8::FunctionCallbackArguments) -> (HRESULT, *mut c_void) {
        let number_of_abi_parameters = self.number_of_abi_parameters;

        let mut arguments: Vec<*mut c_void> = Vec::new();

        arguments.reserve(number_of_abi_parameters);

        unsafe { arguments.push(self.interface.as_raw()) };

        let mut string_buf: Vec<HSTRING> = Vec::new();

        for (i, parameter) in self.parameters.iter().enumerate() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata().unwrap();

            let signature = Signature::to_string(metadata, &type_);

            println!("sig {}", signature.as_str());

            match signature.as_str() {
                "String" => {
                    let string = args.holder().to_string(scope).unwrap();

                    let string = HSTRING::from(string.to_rust_string_lossy(scope));;

                    arguments.push(
                        addr_of!(string) as *mut c_void
                    );

                    string_buf.push(string);
                }
                "Boolean" => {
                    let value = args.holder().boolean_value(scope);
                    // arguments.push(
                    //     addr_of!(value) as *mut c_void
                    // )

                    arguments.push(
                        unsafe { std::mem::transmute(&value) }
                    )
                }
                _ => {
                    let value = args.holder();

                    if value.is_object() {
                        let value = value.to_object(scope).unwrap();

                        let dec = value.get_internal_field(scope, 0).unwrap();

                        let dec = unsafe { v8::Local::<v8::External>::cast(dec) };

                        let dec = dec.value() as *mut DeclarationFFI;

                        let dec = unsafe { &*dec };

                        let instance = dec.instance.clone();
                        match instance {
                            None => {
                                arguments.push(std::ptr::null_mut());
                            }
                            Some(mut instance) => {
                                unsafe { arguments.push(&mut instance.into_raw() as *mut _ as *mut c_void) };
                            }
                        }
                    }
                }
            }
        }

        let mut result: *mut c_void = std::ptr::null_mut();

        let mut a:  MaybeUninit<u32> = MaybeUninit::uninit();

        let mut result_ptr: *mut *mut *mut c_void = &mut addr_of_mut!(result);

        if self.is_initializer {
            arguments.push(result_ptr as *mut c_void);
        } else {
            if !self.is_void {
                match self.return_type.as_str() {
                    "Boolean" | "String" | "UInt32" => {
                        // arguments.push(&mut result as *mut _ as *mut c_void);
                       // arguments.push(addr_of_mut!(result) as *mut _ as *mut c_void);
                        unsafe { arguments.push(mem::transmute(a.as_mut_ptr())); }
                    }
                    _ => {
                        arguments.push(result_ptr as *mut c_void);
                    }
                }
            }
        }

        let mut vtable = self.interface.vtable();

        let mut vtable: *mut *mut c_void = unsafe { std::mem::transmute(vtable) };

        let func = unsafe {
            *vtable.offset(self.index as isize)
        };

        let ret = unsafe {
            call::<i32>(&mut self.cif, CodePtr::from_ptr(func), arguments.as_mut_ptr())
        };

        (HRESULT(ret), result)
    }
}