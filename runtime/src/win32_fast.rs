/// V8 Fast API bindings for Win32 functions with purely numeric signatures.
///
/// `__nsWin32BindFast(dll, fn, retType, [argTypes])` returns a JS `Function`
/// whose template carries fast-call overloads. TurboFan-optimized frames
/// invoke the `fast_call_N` C functions directly — no `FunctionCallbackInfo`,
/// no handle scope, no JS-value boxing. Unoptimized frames use `slow_call`,
/// which must keep identical semantics.
use std::ffi::c_void;
use libffi::middle::{Arg, Cif, CodePtr};
use v8::fast_api::{
    CFunction, CFunctionInfo, CTypeInfo, FastApiCallbackOptions, Flags, Int64Representation,
    Type as FastType,
};

use crate::win32::ffi_type_for;

pub(crate) const MAX_FAST_ARGS: usize = 4;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum FastKind {
    I8,
    I16,
    I32,
    U8,
    U16,
    U32,
    I64,
    U64,
    F32,
    F64,
    Bool,
    Pointer,
    Void,
}

fn fast_kind_for(ty: &str) -> Option<FastKind> {
    Some(match ty {
        "i8" => FastKind::I8,
        "i16" => FastKind::I16,
        "i32" => FastKind::I32,
        "u8" => FastKind::U8,
        "u16" => FastKind::U16,
        "u32" => FastKind::U32,
        "i64" => FastKind::I64,
        "u64" => FastKind::U64,
        "f32" => FastKind::F32,
        "f64" => FastKind::F64,
        "bool" => FastKind::Bool,
        "pointer" => FastKind::Pointer,
        "void" => FastKind::Void,
        _ => return None,
    })
}

/// Resolved pointer + prebuilt Cif + marshaling plan. Boxed and leaked into
/// the function template's External data.
pub(crate) struct BoundWin32Fn {
    fn_ptr: *mut c_void,
    cif: Cif,
    arg_kinds: Vec<FastKind>,
    ret_kind: FastKind,
}

/// Convert a JS number (f64) into an 8-byte argument slot laid out so libffi
/// reads the correct low bytes for the argument's ABI type (x64 little-endian).
#[inline]
fn f64_to_slot(kind: FastKind, v: f64) -> u64 {
    match kind {
        FastKind::I8 => (v as i8) as u8 as u64,
        FastKind::I16 => (v as i16) as u16 as u64,
        FastKind::I32 => (v as i32) as u32 as u64,
        FastKind::U8 => (v as u8) as u64,
        FastKind::U16 => (v as u16) as u64,
        FastKind::U32 => (v as u32) as u64,
        FastKind::I64 => (v as i64) as u64,
        FastKind::U64 | FastKind::Pointer => v as u64,
        FastKind::F32 => (v as f32).to_bits() as u64,
        FastKind::F64 => v.to_bits(),
        FastKind::Bool => (v != 0.0) as u64,
        FastKind::Void => 0,
    }
}

impl BoundWin32Fn {
    /// Shared by the fast C entry points and the slow V8 callback so
    /// semantics cannot diverge.
    #[inline]
    fn invoke(&self, args: &[f64]) -> f64 {
        let mut slots = [0u64; MAX_FAST_ARGS];
        let n = self.arg_kinds.len().min(args.len()).min(MAX_FAST_ARGS);
        for i in 0..n {
            slots[i] = f64_to_slot(self.arg_kinds[i], args[i]);
        }
        let mut call_args: [std::mem::MaybeUninit<Arg>; MAX_FAST_ARGS] =
            [const { std::mem::MaybeUninit::uninit() }; MAX_FAST_ARGS];
        for i in 0..n {
            call_args[i].write(Arg::new(&slots[i]));
        }
        // SAFETY: the first n entries were just initialized.
        let call_args: &[Arg] =
            unsafe { std::slice::from_raw_parts(call_args.as_ptr() as *const Arg, n) };

        let code = CodePtr(self.fn_ptr);
        unsafe {
            match self.ret_kind {
                FastKind::Void => {
                    self.cif.call::<()>(code, call_args);
                    0.0
                }
                FastKind::Bool => (self.cif.call::<i32>(code, call_args) != 0) as i32 as f64,
                FastKind::I8 => self.cif.call::<i8>(code, call_args) as f64,
                FastKind::I16 => self.cif.call::<i16>(code, call_args) as f64,
                FastKind::I32 => self.cif.call::<i32>(code, call_args) as f64,
                FastKind::U8 => self.cif.call::<u8>(code, call_args) as f64,
                FastKind::U16 => self.cif.call::<u16>(code, call_args) as f64,
                FastKind::U32 => self.cif.call::<u32>(code, call_args) as f64,
                FastKind::I64 => self.cif.call::<i64>(code, call_args) as f64,
                FastKind::U64 => self.cif.call::<u64>(code, call_args) as f64,
                FastKind::F32 => self.cif.call::<f32>(code, call_args) as f64,
                FastKind::F64 => self.cif.call::<f64>(code, call_args),
                FastKind::Pointer => self.cif.call::<usize>(code, call_args) as f64,
            }
        }
    }
}

#[inline]
unsafe fn bound_from_data(data: v8::Local<v8::Value>) -> Option<&'static BoundWin32Fn> {
    let ext = unsafe { data.cast::<v8::External>() };
    let ptr = ext.value() as *const BoundWin32Fn;
    if ptr.is_null() { None } else { Some(unsafe { &*ptr }) }
}

// Fast entry points, one per arity. V8 calls these directly from optimized
// code; the bound function arrives via FastApiCallbackOptions::data.
unsafe extern "C" fn fast_call_0(
    _recv: v8::Local<v8::Value>,
    opts: *mut FastApiCallbackOptions,
) -> f64 {
    match unsafe { bound_from_data((*opts).data) } {
        Some(b) => b.invoke(&[]),
        None => 0.0,
    }
}

unsafe extern "C" fn fast_call_1(
    _recv: v8::Local<v8::Value>,
    a0: f64,
    opts: *mut FastApiCallbackOptions,
) -> f64 {
    match unsafe { bound_from_data((*opts).data) } {
        Some(b) => b.invoke(&[a0]),
        None => 0.0,
    }
}

unsafe extern "C" fn fast_call_2(
    _recv: v8::Local<v8::Value>,
    a0: f64,
    a1: f64,
    opts: *mut FastApiCallbackOptions,
) -> f64 {
    match unsafe { bound_from_data((*opts).data) } {
        Some(b) => b.invoke(&[a0, a1]),
        None => 0.0,
    }
}

unsafe extern "C" fn fast_call_3(
    _recv: v8::Local<v8::Value>,
    a0: f64,
    a1: f64,
    a2: f64,
    opts: *mut FastApiCallbackOptions,
) -> f64 {
    match unsafe { bound_from_data((*opts).data) } {
        Some(b) => b.invoke(&[a0, a1, a2]),
        None => 0.0,
    }
}

unsafe extern "C" fn fast_call_4(
    _recv: v8::Local<v8::Value>,
    a0: f64,
    a1: f64,
    a2: f64,
    a3: f64,
    opts: *mut FastApiCallbackOptions,
) -> f64 {
    match unsafe { bound_from_data((*opts).data) } {
        Some(b) => b.invoke(&[a0, a1, a2, a3]),
        None => 0.0,
    }
}

/// `CFunctionInfo` embeds a raw pointer to its arg array, blocking `Sync`; the
/// pointees are `'static` immutable arrays only ever read by V8.
struct SyncCFunctionInfo(CFunctionInfo);
unsafe impl Sync for SyncCFunctionInfo {}

const RECV: CTypeInfo = CTypeInfo::new(FastType::V8Value, Flags::empty());
const F64ARG: CTypeInfo = CTypeInfo::new(FastType::Float64, Flags::empty());
const OPTS: CTypeInfo = CTypeInfo::new(FastType::CallbackOptions, Flags::empty());
const RET_F64: CTypeInfo = CTypeInfo::new(FastType::Float64, Flags::empty());

static ARGS_0: [CTypeInfo; 2] = [RECV, OPTS];
static ARGS_1: [CTypeInfo; 3] = [RECV, F64ARG, OPTS];
static ARGS_2: [CTypeInfo; 4] = [RECV, F64ARG, F64ARG, OPTS];
static ARGS_3: [CTypeInfo; 5] = [RECV, F64ARG, F64ARG, F64ARG, OPTS];
static ARGS_4: [CTypeInfo; 6] = [RECV, F64ARG, F64ARG, F64ARG, F64ARG, OPTS];

static INFO_0: SyncCFunctionInfo =
    SyncCFunctionInfo(CFunctionInfo::new(RET_F64, &ARGS_0, Int64Representation::Number));
static INFO_1: SyncCFunctionInfo =
    SyncCFunctionInfo(CFunctionInfo::new(RET_F64, &ARGS_1, Int64Representation::Number));
static INFO_2: SyncCFunctionInfo =
    SyncCFunctionInfo(CFunctionInfo::new(RET_F64, &ARGS_2, Int64Representation::Number));
static INFO_3: SyncCFunctionInfo =
    SyncCFunctionInfo(CFunctionInfo::new(RET_F64, &ARGS_3, Int64Representation::Number));
static INFO_4: SyncCFunctionInfo =
    SyncCFunctionInfo(CFunctionInfo::new(RET_F64, &ARGS_4, Int64Representation::Number));

fn slow_call(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(bound) = (unsafe { bound_from_data(args.data()) }) else {
        retval.set_double(0.0);
        return;
    };
    let mut vals = [0f64; MAX_FAST_ARGS];
    let n = bound.arg_kinds.len().min(MAX_FAST_ARGS);
    for i in 0..n {
        vals[i] = args.get(i as i32).number_value(scope).unwrap_or(0.0);
    }
    retval.set_double(bound.invoke(&vals[..n]));
}

/// Build a fast-call-capable `Function`, or `None` when the signature is not
/// fast-eligible (string args, arity > MAX_FAST_ARGS, unknown types,
/// unresolvable symbol) so the caller keeps its fallback path.
pub(crate) fn make_fast_bound_function<'a>(
    scope: &mut v8::PinScope<'a, '_>,
    dll: &str,
    fn_name: &str,
    ret_type: &str,
    arg_types: &[String],
) -> Option<v8::Local<'a, v8::Function>> {
    if arg_types.len() > MAX_FAST_ARGS {
        return None;
    }
    let ret_kind = fast_kind_for(ret_type)?;
    let mut arg_kinds = Vec::with_capacity(arg_types.len());
    for t in arg_types {
        let k = fast_kind_for(t)?;
        if k == FastKind::Void {
            return None;
        }
        arg_kinds.push(k);
    }

    let fn_ptr = crate::win32::resolve_fn(dll, fn_name).ok()?;
    let ffi_args = arg_types
        .iter()
        .map(|t| ffi_type_for(t).ok())
        .collect::<Option<Vec<_>>>()?;
    let cif = Cif::new(ffi_args, ffi_type_for(ret_type).ok()?);

    let bound = Box::into_raw(Box::new(BoundWin32Fn {
        fn_ptr,
        cif,
        arg_kinds,
        ret_kind,
    }));
    let ext = v8::External::new(scope, bound as *mut c_void);

    let cfn = match arg_types.len() {
        0 => CFunction::new(fast_call_0 as *const c_void, &INFO_0.0),
        1 => CFunction::new(fast_call_1 as *const c_void, &INFO_1.0),
        2 => CFunction::new(fast_call_2 as *const c_void, &INFO_2.0),
        3 => CFunction::new(fast_call_3 as *const c_void, &INFO_3.0),
        _ => CFunction::new(fast_call_4 as *const c_void, &INFO_4.0),
    };

    let tmpl = v8::FunctionTemplate::builder(slow_call)
        .data(ext.into())
        .build_fast(scope, &[cfn]);
    tmpl.get_function(scope)
}
