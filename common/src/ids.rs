use std::{str::FromStr, fmt::Display};


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct SourceId(u8);

impl SourceId {
    pub fn new(id: u8) -> SourceId {
        SourceId(id)
    }
}

impl Display for SourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}


