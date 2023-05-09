use std::borrow::Cow;
use windows::Win32::System::WinRT::Metadata::{CorElementType, CorTokenType, IMetaDataImport2, MDTypeRefToDef};
use crate::prelude::PCCOR_SIGNATURE;

pub mod com_helpers;
pub mod declarations;
pub mod prelude;
pub mod meta_data_reader;
pub mod value;
pub mod signature;
pub mod generic_instance_id_builder;
pub mod declaration_factory;