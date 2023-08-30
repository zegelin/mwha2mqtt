
use std::{ops::RangeInclusive, num::ParseIntError};
use std::fmt::Display;
use std::str::FromStr;

use serde::{Serialize, Deserialize};
use strum_macros::{EnumDiscriminants, Display, EnumVariantNames, EnumIter};

use thiserror::Error;

use heck::ToKebabCase;

pub const MAX_AMPS: u8 = 3;
pub const MAX_ZONES_PER_AMP: u8 = 6;

pub mod ranges {
    use std::ops::RangeInclusive;

    pub const VOLUME: RangeInclusive<u8> = 0..=38;
    pub const TREBLE: RangeInclusive<u8> = 0..=14;
    pub const BASS: RangeInclusive<u8> = 0..=14;
    pub const BALANCE: RangeInclusive<u8> = 0..=20;
    pub const SOURCE: RangeInclusive<u8> = 1..=6;
}

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

#[derive(Error, Debug)]
pub enum ZoneAttributeError {
    #[error("{attr} value is out of range {range:?}")]
    ValueOutOfRange {
        attr: ZoneAttribute,
        range: RangeInclusive<u8>
    }
}

impl ZoneAttribute {
    pub fn validate(&self) -> Result<(), ZoneAttributeError> {
        use ZoneAttribute::*;

        let (v, range) = match self {
            Volume(v) => (v, ranges::VOLUME),
            Treble(v) => (v, ranges::TREBLE),
            Bass(v) => (v, ranges::BASS),
            Balance(v) => (v, ranges::BALANCE),
            Source(v) => (v, ranges::SOURCE),
            _ => return Ok(()) // boolean attributes are always valid
        };

        if !range.contains(&v) {
            Err(ZoneAttributeError::ValueOutOfRange{ attr: *self, range: range })
            
        } else {
            Ok(())
        }
    }
}

impl ZoneAttributeDiscriminants {
    pub fn read_only(&self) -> bool {
        use ZoneAttributeDiscriminants::*;

        match self {
            PublicAnnouncement => true,
            KeypadConnected => true,
            _ => false,
        }
    }

    pub fn mqtt_set_topic(&self, topic_base: &str, zone: &ZoneId) -> String {
        let attr_name = self.to_string().to_kebab_case();
        format!("{}set/zone/{}/{}", topic_base, zone, attr_name)
    }

    pub fn mqtt_status_topic(&self, topic_base: &str, zone: &ZoneId) -> String {
        let attr_name = self.to_string().to_kebab_case();
        format!("{}status/zone/{}/{}", topic_base, zone, attr_name)
    }
}


#[derive(Error, Debug)]
pub enum ZoneIdError {
    #[error("amp is out of range ([1, {}]) for zone id {0:02}", MAX_AMPS)]
    AmpOutOfRange(u8),

    #[error("zone is out of range ([1, {}]) for zone id {0:02}", MAX_ZONES_PER_AMP)]
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
    Amp(u8),
    System
}

impl ZoneId {
    pub fn to_amps(&self) -> Vec<ZoneId> {
        match *self {
            ZoneId::Zone { amp, zone: _ } => vec![ZoneId::Amp(amp)],
            ZoneId::Amp(amp) => vec![ZoneId::Amp(amp)],
            ZoneId::System => (1..=MAX_AMPS).map(ZoneId::Amp).collect(),
        }
    }

    pub fn to_zones(&self) -> Vec<ZoneId> {
        match *self {
            ZoneId::Zone { amp, zone } => vec![ZoneId::Zone { amp, zone }],
            ZoneId::Amp(amp) => (1..=MAX_ZONES_PER_AMP).map(|zone| ZoneId::Zone { amp, zone }).collect(),
            ZoneId::System => self.to_amps().into_iter().flat_map(|amp| ZoneId::to_zones(&amp)).collect()
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

        if amp == 0 && zone == 0 {
            return Ok(ZoneId::System);
        }

        let amp = match amp {
            1..=MAX_AMPS => amp,
            _ => return Err(ZoneIdError::AmpOutOfRange(value))
        };

        match zone {
            0 => Ok(ZoneId::Amp(amp)),
            1..=MAX_ZONES_PER_AMP  => Ok(ZoneId::Zone { amp, zone }),
            _ => Err(ZoneIdError::ZoneOutOfRange(value))
        }
    }
}

impl From<&ZoneId> for u8 {
    fn from(value: &ZoneId) -> Self {
        let (amp, zone) = match value {
            ZoneId::Zone { amp, zone } => (*amp, *zone),
            ZoneId::Amp(amp) => (*amp, 0),
            ZoneId::System => (0, 0),
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

impl Ord for ZoneId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        u8::from(self).cmp(&other.into())
    }
}

impl PartialOrd for ZoneId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Serialize for ZoneId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl <'de>Deserialize<'de> for ZoneId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>
    {
        // struct StringOrStruct<T>();

        // impl<'de, T> Visitor<'de> for StringOrStruct<T>
        // where
        //     T: Deserialize<'de> + FromStr<Err = Void>,
        // {
        //     type Value = T;

        //     fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        //         formatter.write_str("string or map")
        //     }

        //     fn visit_str<E>(self, value: &str) -> Result<T, E>
        //     where
        //         E: de::Error
        //     {
        //         Ok(FromStr::from_str(value).unwrap())
        //     }

        //     fn visit_map<M>(self, map: M) -> Result<T, M::Error>
        //     where
        //         M: MapAccess<'de>
        //     {
        //         Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        //     }
        // }

        // deserializer.deserialize_any(StringOrStruct())

        todo!()
    }
}


// pub struct ZoneStatus {
//     public_announcement: bool,
//     power: bool,
//     mute: bool,
//     do_not_disturb: bool,
//     volume: u8,
//     treble: u8,
//     bass: u8,
//     balance: u8,
//     source: u8,
//     keypad_connected: bool
// }

// impl From<ZoneStatus> for Vec<ZoneAttribute> {
//     fn from(v: ZoneStatus) -> Self {
//         use ZoneAttribute::*;

//         vec![
//             PublicAnnouncement(v.public_announcement),
//             Power(v.power)
//         ]
//     }
// }

// impl TryFrom<IntoIterator<ZoneAttribute>> for ZoneStatus {



// }

