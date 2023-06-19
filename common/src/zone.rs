
use std::ops::RangeInclusive;
use std::fmt::Display;
use std::str::FromStr;

use serde::Serialize;
use strum_macros::{EnumDiscriminants, Display, EnumVariantNames, EnumIter};


#[derive(Copy, Clone, Debug, Eq, PartialEq, EnumDiscriminants)]
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
            ZoneAttributeDiscriminants::Power => false,
            ZoneAttributeDiscriminants::Mute => false,
            ZoneAttributeDiscriminants::DoNotDisturb => false,
            ZoneAttributeDiscriminants::Volume => false,
            ZoneAttributeDiscriminants::Treble => false,
            ZoneAttributeDiscriminants::Bass => false,
            ZoneAttributeDiscriminants::Balance => false,
            ZoneAttributeDiscriminants::Source => false,
            ZoneAttributeDiscriminants::KeypadConnected => true,
        }
    }

    pub fn io_range(&self) -> std::ops::RangeInclusive<u8> {
        match self {
            ZoneAttributeDiscriminants::PublicAnnouncement => 0..=1,
            ZoneAttributeDiscriminants::Power => 0..=1,
            ZoneAttributeDiscriminants::Mute => 0..=1,
            ZoneAttributeDiscriminants::DoNotDisturb => 0..=1,
            ZoneAttributeDiscriminants::Volume => 0..=38,
            ZoneAttributeDiscriminants::Treble => 0..=14,
            ZoneAttributeDiscriminants::Bass => 0..=14,
            ZoneAttributeDiscriminants::Balance => 0..=20,
            ZoneAttributeDiscriminants::Source => 1..=6,
            ZoneAttributeDiscriminants::KeypadConnected => 0..=1,
        }
    }
}

// impl ZoneAttribute {
//     fn validate(&self) {
//         match self {
//             ZoneAttribute::PublicAnnouncement(v) => todo!(),
//             ZoneAttribute::Power(_) => todo!(),
//             ZoneAttribute::Mute(_) => todo!(),
//             ZoneAttribute::DoNotDisturb(_) => todo!(),
//             ZoneAttribute::Volume(_) => todo!(),
//             ZoneAttribute::Treble(_) => todo!(),
//             ZoneAttribute::Bass(_) => todo!(),
//             ZoneAttribute::Balance(_) => todo!(),
//             ZoneAttribute::Source(_) => todo!(),
//             ZoneAttribute::KeypadConnected(_) => todo!(),
//         }
//     }
// }


#[derive(Debug, PartialEq, Eq)]
pub struct InvalidZoneIdError;

impl std::error::Error for InvalidZoneIdError {}

impl Display for InvalidZoneIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}





#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ZoneId {
    Zone { amp: u8, zone: u8 },
    Amp(u8)
}

impl ZoneId {
    pub fn zone(amp: u8, zone: u8) -> Result<ZoneId, InvalidZoneIdError> {
        ZoneId::amp(amp)?; // validate amp value

        match zone {
            0 => Ok(ZoneId::Amp(amp)),
            1..=6  => Ok(ZoneId::Zone { amp, zone }),
            _ => Err(InvalidZoneIdError)
        }
    }

    pub fn amp(amp: u8) -> Result<ZoneId, InvalidZoneIdError> {
        match amp {
            1..=3 => Ok(ZoneId::Amp(amp)),
            _ => Err(InvalidZoneIdError{})
        }
    }

    pub fn to_amp(&self) -> ZoneId {
        match *self {
            ZoneId::Zone { amp, zone } => ZoneId::Amp(amp),
            other => other
        }
    }
}

impl FromStr for ZoneId {
    type Err = InvalidZoneIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        todo!()
    }
}

impl TryFrom<u8> for ZoneId {
    type Error = InvalidZoneIdError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let amp = value / 10;
        let zone = value % 10;

        ZoneId::zone(amp, zone)
    }
}

impl Display for ZoneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (amp, zone) = match *self {
            ZoneId::Zone { amp, zone } => (amp, zone),
            ZoneId::Amp(amp) => (amp, 0),
        };
        
        write!(f, "{}{}", amp, zone)
    }
}

impl Serialize for ZoneId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        todo!();
        // serializer.ser
    }
}