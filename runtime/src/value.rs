use std::ffi::c_void;
use std::mem::MaybeUninit;
use std::ptr::{addr_of, addr_of_mut};
use std::sync::Arc;
use parking_lot::RwLock;
use windows::core::{ComInterface, GUID, HRESULT, HSTRING, Interface, IUnknown};
use metadata::declarations::declaration::Declaration;
use libffi::low::*;
use libffi::raw::ffi_call;
use v8::FunctionCallbackArguments;
use windows::Win32::System::WinRT::IActivationFactory;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::declarations::method_declaration::MethodDeclaration;
use metadata::declarations::parameter_declaration::ParameterDeclaration;
use metadata::declaring_interface_for_method::Metadata;
use metadata::signature::Signature;

pub enum Value {
    Constructor(Arc<RwLock<dyn Declaration>>),
    Object(Arc<RwLock<dyn Declaration>>)
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
    parameters: Vec<ParameterDeclaration>
}

impl MethodCall {
    pub fn new(method: &MethodDeclaration, is_sealed: bool, interface: IUnknown, is_initializer: bool) -> MethodCall {

        let number_of_parameters = method.number_of_parameters();

        let mut index = 0 as usize;

        let iid = match Metadata::find_declaring_interface_for_method(method, &mut index) {
            None => {
                index = 0;
                IActivationFactory::IID
            }
            Some(interface) => {
                let ii_lock = interface.read();
                let ii = ii_lock.as_declaration().as_any().downcast_ref::<InterfaceDeclaration>();
                let ii = ii.unwrap();
                ii.id()
            }
        };

        index = index.saturating_add(6); // account for IInspectable vtable overhead

        let mut interface_ptr: *mut c_void = std::ptr::null_mut(); // IActivationFactory

        let vtable = interface.vtable();

        let interface_ptr_ptr = addr_of_mut!(interface_ptr);

        let result = unsafe { ((*vtable).QueryInterface)(interface.as_raw(), &iid, interface_ptr_ptr as *mut *const c_void)};

        assert!(result.is_ok());

        let is_composition = !is_sealed;

        let is_void = method.is_void();

        let other_params: usize = if is_initializer {
            if is_sealed { 2 } else { 4 }
        }else {
            if is_void {1} else {2}
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
                _ => {}
            }
        }

        if is_initializer {
            if is_composition {
                unsafe { parameter_types.push(&mut types::pointer); }
                unsafe { parameter_types.push(&mut types::pointer); }
            }

            unsafe { parameter_types.push(&mut types::pointer); }
        }else {
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

        // todo handle prep_cif error

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
        }

    }

    pub fn call(&mut self,scope: &mut v8::HandleScope, args: &FunctionCallbackArguments) -> (HRESULT, *mut c_void) {

        let number_of_abi_parameters = self.number_of_abi_parameters;

        let mut arguments: Vec<*mut c_void> = Vec::new();

        arguments.reserve(number_of_abi_parameters);

        unsafe { arguments.push(self.interface.as_raw()) };

        let mut string_buf = Vec::new();

        for (i, parameter) in self.parameters.iter().enumerate() {
            let type_ = parameter.type_();
            let metadata = parameter.metadata().unwrap();

            let signature = Signature::to_string(metadata, &type_);

            match signature.as_str() {
                "String" => {
                    let string = args.get(i as i32).to_string(scope).unwrap();

                    let string = HSTRING::from(string.to_rust_string_lossy(scope));

                    arguments.push(
                        addr_of!(string) as *mut c_void
                    );

                    string_buf.push(string);
                }
                _ => {}
            }
        }

        let mut result: *mut c_void = std::ptr::null_mut();

        let mut result_ptr: *mut *mut *mut c_void = &mut addr_of_mut!(result);

        if self.is_initializer {
            arguments.push(result_ptr as *mut c_void);
        }else {
            if !self.is_void {
                arguments.push(result_ptr as *mut c_void);
            }
        }


        let mut vtable = self.interface.vtable();

        let mut vtable: *mut *mut c_void = unsafe { std::mem::transmute(vtable)};

        let func = unsafe {
            *vtable.offset(self.index as isize)
        };

        let ret = unsafe {
            call::<i32>(&mut self.cif, CodePtr::from_ptr(func), arguments.as_mut_ptr())
        };

        (HRESULT(ret), result)
    }
}