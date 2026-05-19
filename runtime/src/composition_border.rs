use std::ffi::c_void;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicI64, Ordering};

use parking_lot::Mutex;
use windows::core::{IUnknown, Interface, GUID, HRESULT, HSTRING};
use windows::UI::Composition::{
    CompositionColorBrush, ContainerVisual, SpriteVisual, Visual,
};
use windows::Win32::System::WinRT::RoGetActivationFactory;

use metadata::declarations::declaration::DeclarationKind;
use metadata::declarations::interface_declaration::InterfaceDeclaration;
use metadata::meta_data_reader::MetadataReader;

use crate::error::{AnyError, generic_error};

fn iid_ui_element() -> GUID {
    static IID: OnceLock<GUID> = OnceLock::new();
    *IID.get_or_init(|| {
        MetadataReader::find_by_name("Windows.UI.Xaml.IUIElement")
            .and_then(|d| {
                let lock = d.read();
                if lock.kind() == DeclarationKind::Interface {
                    lock.as_any().downcast_ref::<InterfaceDeclaration>().map(|i| i.id())
                } else { None }
            })
            .unwrap_or(GUID::zeroed())
    })
}

fn iid_ecp_statics() -> GUID {
    static IID: OnceLock<GUID> = OnceLock::new();
    *IID.get_or_init(|| {
        MetadataReader::find_by_name("Windows.UI.Xaml.Hosting.IElementCompositionPreviewStatics")
            .and_then(|d| {
                let lock = d.read();
                if lock.kind() == DeclarationKind::Interface {
                    lock.as_any().downcast_ref::<InterfaceDeclaration>().map(|i| i.id())
                } else { None }
            })
            .unwrap_or(GUID::zeroed())
    })
}

const SLOT_QI: usize = 0;
const SLOT_RELEASE: usize = 2;
const SLOT_ECP_GET_ELEMENT_VISUAL: usize = 6;
const SLOT_ECP_SET_ELEMENT_CHILD_VISUAL: usize = 8;

type QiFn = unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT;
type RelFn = unsafe extern "system" fn(*mut c_void) -> u32;
type GetVisualFn = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> HRESULT;
type SetChildFn = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> HRESULT;

unsafe fn vtable_of(ptr: *mut c_void) -> *const *const c_void {
    *(ptr as *const *const *const c_void)
}

unsafe fn com_qi(ptr: *mut c_void, iid: &GUID) -> Option<*mut c_void> {
    if ptr.is_null() { return None; }
    let vtbl = vtable_of(ptr);
    let f: QiFn = std::mem::transmute(*vtbl.add(SLOT_QI));
    let mut out: *mut c_void = std::ptr::null_mut();
    if f(ptr, iid, &mut out).is_ok() && !out.is_null() { Some(out) } else { None }
}

unsafe fn com_release(ptr: *mut c_void) {
    if ptr.is_null() { return; }
    let vtbl = vtable_of(ptr);
    let f: RelFn = std::mem::transmute(*vtbl.add(SLOT_RELEASE));
    f(ptr);
}

fn ecp_get_element_visual(element_ptr: *mut c_void) -> Result<Visual, AnyError> {
    unsafe {
        let name = HSTRING::from("Windows.UI.Xaml.Hosting.ElementCompositionPreview");
        let factory = RoGetActivationFactory::<IUnknown>(&name)
            .map_err(|e| generic_error(format!("ECP factory: {:?}", e)))?;

        let statics = com_qi(factory.as_raw() as *mut c_void, &iid_ecp_statics())
            .ok_or_else(|| generic_error("QI IElementCompositionPreviewStatics failed"))?;

        let ui_elem = match com_qi(element_ptr, &iid_ui_element()) {
            Some(p) => p,
            None => { com_release(statics); return Err(generic_error("not a UIElement")); }
        };

        let vtbl = vtable_of(statics);
        let get_fn: GetVisualFn = std::mem::transmute(*vtbl.add(SLOT_ECP_GET_ELEMENT_VISUAL));

        let mut visual_ptr: *mut c_void = std::ptr::null_mut();
        let hr = get_fn(statics, ui_elem, &mut visual_ptr);
        com_release(ui_elem);
        com_release(statics);

        hr.ok().map_err(|e| generic_error(format!("GetElementVisual: {:?}", e)))?;
        if visual_ptr.is_null() { return Err(generic_error("GetElementVisual returned null")); }
        Ok(Visual::from_raw(visual_ptr as *mut _))
    }
}

fn ecp_set_element_child_visual(
    element_ptr: *mut c_void,
    visual_ptr: Option<*mut c_void>,
) -> Result<(), AnyError> {
    unsafe {
        let name = HSTRING::from("Windows.UI.Xaml.Hosting.ElementCompositionPreview");
        let factory = RoGetActivationFactory::<IUnknown>(&name)
            .map_err(|e| generic_error(format!("ECP factory: {:?}", e)))?;

        let statics = com_qi(factory.as_raw() as *mut c_void, &iid_ecp_statics())
            .ok_or_else(|| generic_error("QI IElementCompositionPreviewStatics failed"))?;

        let ui_elem = match com_qi(element_ptr, &iid_ui_element()) {
            Some(p) => p,
            None => { com_release(statics); return Err(generic_error("not a UIElement")); }
        };

        let vtbl = vtable_of(statics);
        let set_fn: SetChildFn = std::mem::transmute(*vtbl.add(SLOT_ECP_SET_ELEMENT_CHILD_VISUAL));
        let hr = set_fn(statics, ui_elem, visual_ptr.unwrap_or(std::ptr::null_mut()));
        com_release(ui_elem);
        com_release(statics);

        hr.ok().map_err(|e| generic_error(format!("SetElementChildVisual: {:?}", e)))?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct RawComPtr(*mut c_void);
unsafe impl Send for RawComPtr {}
unsafe impl Sync for RawComPtr {}

static BORDER_CONTAINERS: OnceLock<Mutex<HashMap<usize, RawComPtr>>> = OnceLock::new();
static INSTANCES: OnceLock<Mutex<HashMap<i64, BorderInstance>>> = OnceLock::new();
static NEXT_INSTANCE_ID: AtomicI64 = AtomicI64::new(1);

struct BorderInstance {
    container_raw: *mut c_void,
    left_raw: *mut c_void,
    top_raw: *mut c_void,
    right_raw: *mut c_void,
    bottom_raw: *mut c_void,
    brush_raw: *mut c_void,
    element_raw: *mut c_void,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    color: u32,
    radius_tl: f32,
    radius_tr: f32,
    radius_br: f32,
    radius_bl: f32,
}

unsafe impl Send for BorderInstance {}
unsafe impl Sync for BorderInstance {}

fn parse_color(argb: u32) -> windows::UI::Color {
    windows::UI::Color {
        A: ((argb >> 24) & 0xff) as u8,
        R: ((argb >> 16) & 0xff) as u8,
        G: ((argb >> 8) & 0xff) as u8,
        B: (argb & 0xff) as u8,
    }
}

pub fn ensure_container_for_element(element_ptr: *mut c_void) -> Result<i64, AnyError> {
    if element_ptr.is_null() { return Err(generic_error("null element")); }

    let key = element_ptr as usize;
    let map = BORDER_CONTAINERS.get_or_init(|| Mutex::new(HashMap::new()));

    if let Some(raw) = map.lock().get(&key) {
        return Ok(raw.0 as i64);
    }

    let root_visual = ecp_get_element_visual(element_ptr)?;
    let compositor = root_visual.Compositor()
        .map_err(|e| generic_error(format!("Compositor: {:?}", e)))?;
    let container = compositor.CreateContainerVisual()
        .map_err(|e| generic_error(format!("CreateContainerVisual: {:?}", e)))?;

    if let Ok(anim) = compositor.CreateExpressionAnimation() {
        let _ = anim.SetExpression(&HSTRING::from("root.Size"));
        let _ = anim.SetReferenceParameter(&HSTRING::from("root"), &root_visual);
        let _ = container.StartAnimation(&HSTRING::from("Size"), &anim);
    }

    let as_visual: Visual = container.cast()
        .map_err(|e| generic_error(format!("cast to Visual: {:?}", e)))?;
    ecp_set_element_child_visual(element_ptr, Some(as_visual.as_raw() as *mut c_void))?;

    let raw = container.as_raw() as *mut c_void;
    std::mem::forget(container);
    map.lock().insert(key, RawComPtr(raw));
    Ok(raw as i64)
}

pub fn create_border_instance(element_ptr: *mut c_void) -> Result<i64, AnyError> {
    if element_ptr.is_null() { return Err(generic_error("null element")); }

    let container_raw = ensure_container_for_element(element_ptr)? as *mut c_void;

    unsafe {
        let container = ContainerVisual::from_raw(container_raw as *mut _);
        let compositor = container.Compositor()
            .map_err(|e| generic_error(format!("Compositor: {:?}", e)))?;

        let mk_sprite = || compositor.CreateSpriteVisual()
            .map_err(|e| generic_error(format!("CreateSpriteVisual: {:?}", e)));

        let left = mk_sprite()?;
        let top = mk_sprite()?;
        let right = mk_sprite()?;
        let bottom = mk_sprite()?;

        let brush = compositor.CreateColorBrush()
            .map_err(|e| generic_error(format!("CreateColorBrush: {:?}", e)))?;

        left.SetBrush(&brush).ok();
        top.SetBrush(&brush).ok();
        right.SetBrush(&brush).ok();
        bottom.SetBrush(&brush).ok();

        let children = container.Children()
            .map_err(|e| generic_error(format!("Children: {:?}", e)))?;
        children.InsertAtTop(&left).ok();
        children.InsertAtTop(&top).ok();
        children.InsertAtTop(&right).ok();
        children.InsertAtTop(&bottom).ok();

        let left_raw   = left.as_raw()   as *mut c_void; std::mem::forget(left);
        let top_raw    = top.as_raw()    as *mut c_void; std::mem::forget(top);
        let right_raw  = right.as_raw()  as *mut c_void; std::mem::forget(right);
        let bottom_raw = bottom.as_raw() as *mut c_void; std::mem::forget(bottom);
        let brush_raw  = brush.as_raw()  as *mut c_void; std::mem::forget(brush);
        std::mem::forget(container);

        let id = NEXT_INSTANCE_ID.fetch_add(1, Ordering::SeqCst);
        INSTANCES.get_or_init(|| Mutex::new(HashMap::new())).lock().insert(id, BorderInstance {
            container_raw, left_raw, top_raw, right_raw, bottom_raw, brush_raw,
            element_raw: element_ptr,
            left: 0.0, top: 0.0, right: 0.0, bottom: 0.0, color: 0,
            radius_tl: 0.0, radius_tr: 0.0, radius_br: 0.0, radius_bl: 0.0,
        });

        Ok(id)
    }
}

pub fn set_border(
    instance_id: i64,
    left: f32, top: f32, right: f32, bottom: f32,
    color: u32,
    radius_tl: f32, radius_tr: f32, radius_br: f32, radius_bl: f32,
) -> Result<(), AnyError> {
    let map = INSTANCES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock();
    let inst = guard.get_mut(&instance_id)
        .ok_or_else(|| generic_error("invalid border instance id"))?;

    unsafe {
        let container = ContainerVisual::from_raw(inst.container_raw as *mut _);
        let left_v    = SpriteVisual::from_raw(inst.left_raw   as *mut _);
        let top_v     = SpriteVisual::from_raw(inst.top_raw    as *mut _);
        let right_v   = SpriteVisual::from_raw(inst.right_raw  as *mut _);
        let bottom_v  = SpriteVisual::from_raw(inst.bottom_raw as *mut _);
        let brush     = CompositionColorBrush::from_raw(inst.brush_raw as *mut _);

        brush.SetColor(parse_color(color))
            .map_err(|e| generic_error(format!("SetColor: {:?}", e)))?;

        inst.left = left; inst.top = top; inst.right = right; inst.bottom = bottom;
        inst.color = color;
        inst.radius_tl = radius_tl; inst.radius_tr = radius_tr;
        inst.radius_br = radius_br; inst.radius_bl = radius_bl;

        let compositor = container.Compositor()
            .map_err(|e| generic_error(format!("Compositor: {:?}", e)))?;

        let mk = |expr: &str, prop: &str, target: &SpriteVisual| -> Result<(), AnyError> {
            let a = compositor.CreateExpressionAnimation()
                .map_err(|e| generic_error(format!("CreateExpressionAnimation: {:?}", e)))?;
            a.SetExpression(&HSTRING::from(expr))
                .map_err(|e| generic_error(format!("SetExpression: {:?}", e)))?;
            a.SetReferenceParameter(&HSTRING::from("root"), &container).ok();
            target.StartAnimation(&HSTRING::from(prop), &a).ok();
            Ok(())
        };

        mk(&format!("Vector2({left},root.Size.Y)"),        "Size",   &left_v)?;
        mk("Vector3(0,0,0)",                               "Offset", &left_v)?;
        mk(&format!("Vector2({right},root.Size.Y)"),       "Size",   &right_v)?;
        mk(&format!("Vector3(root.Size.X-{right},0,0)"),   "Offset", &right_v)?;
        mk(&format!("Vector2(root.Size.X,{top})"),         "Size",   &top_v)?;
        mk("Vector3(0,0,0)",                               "Offset", &top_v)?;
        mk(&format!("Vector2(root.Size.X,{bottom})"),      "Size",   &bottom_v)?;
        mk(&format!("Vector3(0,root.Size.Y-{bottom},0)"),  "Offset", &bottom_v)?;

        std::mem::forget(container);
        std::mem::forget(left_v); std::mem::forget(top_v);
        std::mem::forget(right_v); std::mem::forget(bottom_v);
        std::mem::forget(brush);
    }

    Ok(())
}

pub fn get_border_params(
    instance_id: i64,
) -> Result<(f32, f32, f32, f32, u32, f32, f32, f32, f32), AnyError> {
    let guard = INSTANCES.get_or_init(|| Mutex::new(HashMap::new())).lock();
    let inst = guard.get(&instance_id)
        .ok_or_else(|| generic_error("invalid border instance id"))?;
    Ok((inst.left, inst.top, inst.right, inst.bottom, inst.color,
        inst.radius_tl, inst.radius_tr, inst.radius_br, inst.radius_bl))
}

pub fn try_redraw_border(_instance_id: i64) -> Result<(), AnyError> {
    Ok(())
}

pub fn free_border_instance(instance_id: i64) -> Result<(), AnyError> {
    let map = INSTANCES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock();
    if let Some(inst) = guard.remove(&instance_id) {
        let element_ptr = inst.element_raw;
        let still_used = guard.values().any(|i| i.element_raw == element_ptr);
        if !still_used {
            let containers = BORDER_CONTAINERS.get_or_init(|| Mutex::new(HashMap::new()));
            if containers.lock().remove(&(element_ptr as usize)).is_some() {
                let _ = ecp_set_element_child_visual(element_ptr, None);
            }
        }
        unsafe {
            let _ = ContainerVisual::from_raw(inst.container_raw as *mut _);
            let _ = SpriteVisual::from_raw(inst.left_raw   as *mut _);
            let _ = SpriteVisual::from_raw(inst.top_raw    as *mut _);
            let _ = SpriteVisual::from_raw(inst.right_raw  as *mut _);
            let _ = SpriteVisual::from_raw(inst.bottom_raw as *mut _);
            let _ = CompositionColorBrush::from_raw(inst.brush_raw as *mut _);
        }
    }
    Ok(())
}
