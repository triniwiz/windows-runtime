use regex::Regex;

pub struct GenericReturnTypes<'s> {
    names: Vec<&'s str>,
    types: usize,
}

impl GenericReturnTypes<'_> {
    pub fn names(&self) -> &[&str] {
        self.names.as_slice()
    }

    pub fn types(&self) -> usize {
        self.types
    }
}

pub fn get_generic_return_types(name: &str) -> GenericReturnTypes {
    let types = match Regex::new(r"`(\d+)") {
        Ok(types) => {
            if let Some(captures) = types.captures(name) {
                captures.get(1).unwrap().as_str().parse::<usize>().unwrap()
            } else {
                0
            }
        }
        Err(_) => 0,
    };

    let names = match Regex::new(r"<(.*?)>") {
        Ok(names) => {
            if let Some(captures) = names.captures(name) {
                captures.get(1).unwrap().as_str().split(", ").collect::<Vec<_>>()
            } else {
                vec![]
            }
        }
        Err(_) => vec![],
    };

    GenericReturnTypes { names, types }
}
