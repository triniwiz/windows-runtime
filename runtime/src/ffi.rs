use crate::error::generic_error;
use crate::value::NativeType;
use crate::value::NativeValue;
use libffi::middle::Arg;
use windows::core::HSTRING;

/// Stable storage for HSTRING arguments across a libffi call.
///
/// WinRT requires HSTRING parameters to be passed by-value (handle-sized).
/// The `NativeValue` union stores the HSTRING in a `ManuallyDrop` wrapper
/// that is only valid as long as the `argument_buf` slice is alive. To
/// guarantee the handle remains valid through the `cif.call` boundary we
/// clone each HSTRING into `string_clones` (stable heap allocation) and
/// extract the raw handle word to pass through libffi.
pub struct FfiStringPrep {
    /// Cloned HSTRING wrappers whose handles are passed by-value.  Kept
    /// alive until the call completes; dropped afterward.
    pub string_clones: Vec<HSTRING>,
    /// Raw handle values extracted from each clone (one per String slot).
    pub string_handle_values: Vec<usize>,
    /// Maps argument-buffer slot index → index into `string_handle_values`.
    pub string_index_for_slot: Vec<Option<usize>>,
    /// Resolved ABI-effective type for each argument slot.
    pub effective_natives: Vec<NativeType>,
}

impl FfiStringPrep {
    pub fn new(slot_count: usize, string_count: usize) -> Self {
        Self {
            string_clones: Vec::with_capacity(string_count),
            string_handle_values: Vec::with_capacity(string_count),
            string_index_for_slot: vec![None; slot_count],
            effective_natives: Vec::with_capacity(slot_count),
        }
    }
}

/// Build effective ABI native types and stable HSTRING storage for a call.
///
/// For each argument slot:
/// - If the ABI type is `Pointer` but the parse type is `String`, treat it
///   as `String` so the HSTRING handle is passed by-value (not as a pointer
///   to the wrapper).
/// - Clone String arguments into stable storage and capture their raw
///   handle values so `build_call_args` can pass them through libffi.
pub fn prepare_string_storage(
    argument_buf: &[NativeValue],
    parameter_types: &[NativeType],
    argument_parse_types: &[Option<NativeType>],
) -> Result<FfiStringPrep, crate::error::AnyError> {
    let slot_count = argument_buf.len();
    let string_count = argument_parse_types
        .iter()
        .filter(|opt| matches!(opt, Some(NativeType::String)))
        .count();

    // No HSTRING arguments: `build_call_args` falls back to the ABI types
    // directly, so no per-slot bookkeeping is needed.
    if string_count == 0 {
        return Ok(FfiStringPrep::new(0, 0));
    }

    let mut prep = FfiStringPrep::new(slot_count, string_count);

    for (i, v) in argument_buf.iter().enumerate() {
        let abi_native = parameter_types
            .get(i)
            .ok_or_else(|| generic_error("missing abi native type for slot"))?;

        let effective_native = if matches!(abi_native, NativeType::Pointer) {
            if let Some(Some(parse_pt)) = argument_parse_types.get(i) {
                if matches!(parse_pt, NativeType::String) {
                    NativeType::String
                } else {
                    abi_native.clone()
                }
            } else {
                abi_native.clone()
            }
        } else {
            abi_native.clone()
        };

        prep.effective_natives.push(effective_native.clone());

        if matches!(effective_native, NativeType::String) {
            // Clone the HSTRING into stable heap storage and extract the
            // underlying handle word to pass by-value through libffi.
            let h_ref: &HSTRING = unsafe { &*v.string };
            let idx = prep.string_clones.len();
            prep.string_clones.push(h_ref.clone());
            let handle_val: usize = unsafe {
                std::ptr::read_unaligned(&prep.string_clones[idx] as *const HSTRING as *const usize)
            };
            prep.string_handle_values.push(handle_val);
            prep.string_index_for_slot[i] = Some(idx);
        }
    }

    Ok(prep)
}

/// Construct the `libffi::Arg` vector from prepared storage.
///
/// For String slots the argument is the raw HSTRING handle (usize) stored
/// in `prep.string_handle_values`; for all other slots it is a typed
/// reference into `argument_buf`. When `prep` is empty (no string args) the
/// ABI types in `parameter_types` are used directly, borrow-only.
pub fn build_call_args<'a>(
    prep: &'a FfiStringPrep,
    argument_buf: &'a [NativeValue],
    parameter_types: &'a [NativeType],
) -> Vec<Arg<'a>> {
    let mut call_args: Vec<Arg> = Vec::with_capacity(argument_buf.len());

    if prep.effective_natives.is_empty() {
        for (i, v) in argument_buf.iter().enumerate() {
            let abi = parameter_types.get(i).unwrap_or(&POINTER_FALLBACK_REF);
            call_args.push(unsafe { v.as_arg(abi) });
        }
        return call_args;
    }

    for (i, v) in argument_buf.iter().enumerate() {
        if let Some(idx) = prep.string_index_for_slot.get(i).and_then(|o| *o) {
            call_args.push(Arg::new(&prep.string_handle_values[idx]));
        } else {
            let effective = prep
                .effective_natives
                .get(i)
                .unwrap_or(&POINTER_FALLBACK_REF);
            call_args.push(unsafe { v.as_arg(effective) });
        }
    }

    call_args
}

static POINTER_FALLBACK_REF: NativeType = NativeType::Pointer;
