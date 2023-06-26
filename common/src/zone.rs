
use std::{ops::RangeInclusive, num::ParseIntError};
use std::fmt::Display;
use std::str::FromStr;

use strum_macros::{EnumDiscriminants, Display, EnumVariantNames, EnumIter};

use thiserror::Error;

#[derive(Copy, Clone, Debug, Eq, PartialEq, EnumDiscriminants, Display)]
#[strum_discriminants(derive(EnumIter, Display, Hash))]
pub enum ZoneAttribute {
    PublicAnnouncement(bool),
    Power(bool),
    Mute(bool),
    DoNotDisturb(bool),
    Volume(u8),
    Treble(u8),
    Bass(u8),
    Balance(u8),
    Source(u8),
    KeypadConnected(bool)
}

impl ZoneAttributeDiscriminants {
    pub fn read_only(&self) -> bool {
        match self {
            ZoneAttributeDiscriminants::PublicAnnouncement => true,
            ZoneAttributeDiscriminants::KeypadConnected => true,
            _ => false,
        }
    }

    pub fn io_range(&self) -> std::ops::RangeInclusive<u8> {
        match self {
            ZoneAttributeDiscriminants::Volume => 0..=38,
            ZoneAttributeDiscriminants::Treble => 0..=14,
            ZoneAttributeDiscriminants::Bass => 0..=14,
            ZoneAttributeDiscriminants::Balance => 0..=20,
            ZoneAttributeDiscriminants::Source => 1..=6,
            _ => 0..=1, // all other attrs are booleans
        }
    }
}


#[derive(Error, Debug)]
pub enum ZoneIdError {
    #[error("amp is out of range ([1, 3]) for zone id {0:02}")]
    AmpOutOfRange(u8),

    #[error("zone is out of range ([1, 6]) for zone id {0:02}")]
    ZoneOutOfRange(u8),

    #[error("cannot parse \"{value}\" as zone id ({source})")]
    ParseFailure {
        value: String,

         #[source]
         source: ParseIntError,
    }
}


#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ZoneId {
    Zone { amp: u8, zone: u8 },
    Amp(u8)
}

impl ZoneId {
    pub fn to_amp(&self) -> ZoneId {
        match *self {
            ZoneId::Zone { amp, zone: _ } => ZoneId::Amp(amp),
            other => other
        }
    }
}


impl FromStr for ZoneId {
    type Err = ZoneIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let i = s.parse::<u8>().map_err(|e| ZoneIdError::ParseFailure{ value: s.to_string(), source: e })?;
        ZoneId::try_from(i)
    }
}

impl TryFrom<u8> for ZoneId {
    type Error = ZoneIdError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let amp = value / 10;
        let zone = value % 10;

        let amp = match amp {
            1..=3 => amp,
            _ => return Err(ZoneIdError::AmpOutOfRange(value))
        };

        match zone {
            0 => Ok(ZoneId::Amp(amp)),
            1..=6  => Ok(ZoneId::Zone { amp, zone }),
            _ => Err(ZoneIdError::ZoneOutOfRange(value))
        }
    }
}

impl From<&ZoneId> for u8 {
    fn from(value: &ZoneId) -> Self {
        let (amp, zone) = match value {
            ZoneId::Zone { amp, zone } => (*amp, *zone),
            ZoneId::Amp(amp) => (*amp, 0),
        };

        (amp * 10) + zone
    }
}

impl Display for ZoneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id: u8 = self.into();
        
        write!(f, "{:02}", id)
    }
}