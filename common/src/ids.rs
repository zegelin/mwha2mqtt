use std::{str::FromStr, fmt::Display, num::ParseIntError};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SourceIdError {
    #[error("source id {0} is out of range [1,6]")]
    OutOfRange(u8),

    #[error("cannot parse \"{value}\" as source id ({source})")]
    ParseFailure {
        value: String,

         #[source]
         source: ParseIntError,
    }
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct SourceId(u8);

impl FromStr for SourceId {
    type Err = SourceIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let i = s.parse::<u8>().map_err(|e| SourceIdError::ParseFailure{ value: s.to_string(), source: e })?;
        SourceId::try_from(i)
    }
}


impl TryFrom<u8> for SourceId {
    type Error = SourceIdError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1..=6 => Ok(SourceId(value)),
            _ => Err(SourceIdError::OutOfRange(value))
        }
    }
}

impl From<&SourceId> for u8 {
    fn from(value: &SourceId) -> Self {
        value.0
    }
}

impl Display for SourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}