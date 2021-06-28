use crate::metadata::meta_data_reader::MetadataReader;
use crate::prelude::{ get_lock_value};

#[derive(Debug)]
pub struct COMInterop {}

impl COMInterop {
	pub fn new() -> Self {
		Self {}
	}
	pub fn resolve_type(type_name: &str) {
		let decl = MetadataReader::find_by_name(type_name);
		println!("{:?}",decl);
		/*if let Some(decl) = decl {
			if let Some(decl) = decl.try_read() {

			}
			decl.try_read().unwrap()
			let val = get_lock_value(decl);
			val.kind()
		}
		*/
	}
}