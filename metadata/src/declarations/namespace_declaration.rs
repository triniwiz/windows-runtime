use std::any::Any;
use std::mem::MaybeUninit;
use windows::core::HSTRING;
use windows::Win32::System::WinRT::Metadata::{CorTokenType, RoResolveNamespace};
use crate::declarations::declaration::{Declaration, DeclarationKind};
use crate::declarations::type_declaration::TypeDeclaration;

#[derive(Clone, Debug)]
pub struct NamespaceDeclaration {
    base: TypeDeclaration,
    children: Vec<String>,
    full_name: String,
    name: String
}

impl Declaration for NamespaceDeclaration {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn name(&self) -> &str {
        return self.name.as_str();
    }

    fn full_name(&self) -> &str {
        self.full_name.as_str()
    }

    fn kind(&self) -> DeclarationKind {
        self.base.kind()
    }
}

impl NamespaceDeclaration {
    pub fn new(full_name: &str) -> Self {
        let mut namespaces_count = 0;
        let none = HSTRING::default();

        // Grab size
        let name = windows::core::HSTRING::from(full_name);
        let mut spaces: MaybeUninit<*mut HSTRING> = MaybeUninit::zeroed();
        unsafe {
            let result = RoResolveNamespace(
                &name,
                &none,
                None,
                None,
                None,
                Some(&mut namespaces_count),
                Some(spaces.as_mut_ptr())
            );
        }


        let namespaces = unsafe{spaces.assume_init()};
        let namespaces = unsafe{std::slice::from_raw_parts(namespaces, namespaces_count as usize).to_vec()};


        let children: Vec<_> = unsafe {
            namespaces
                .into_iter()
                .map(|val| {
                    val.to_string_lossy()
                })
                .collect()
        };
        // The search for the "Windows" namespace on Windows Phone 8.1 fails both on a device and on an emulator with corrupted metadata error.

        // if (FAILED(hr)) {
        // 	return;
        // }

        let mut name = full_name.to_string();
        if let Some(index) = name.find('.') {
            name = name.chars().skip(index + 1).collect()
        }

        Self {
            base: TypeDeclaration::new(DeclarationKind::Namespace, None, CorTokenType::default()),
            children,
            full_name: full_name.to_string(),
            name
        }
    }
    pub fn children(&self) -> &[String] {
        self.children.as_slice()
    }
}
