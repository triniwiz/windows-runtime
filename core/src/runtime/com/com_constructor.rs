use crate::prelude::*;
use crate::metadata::declaring_interface_for_method::Metadata;


#[derive(Debug)]
pub struct COMConstructor {}

#[derive(Debug)]
pub struct COMConstructorCall {
	initialization: Initialization,
}

impl COMConstructorCall {
	pub fn new() -> Self {
		Metadata::find_declaring_interface_for_method()
	}
}