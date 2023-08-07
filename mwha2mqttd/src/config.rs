use std::{io, path::PathBuf, collections::HashMap, time::Duration, str::FromStr, marker::PhantomData, fmt::{self, Display}, hash::Hash, num::ParseIntError};

use figment::{Figment, providers::{Format, Toml}, value::magic::RelativePathBuf};
use serde::{Deserialize, Deserializer, de::{Visitor, self, MapAccess}};

use void::Void;

use anyhow::Result;

use thiserror::Error;

use crate::serial::{BaudConfig, BAUD_RATES, AdjustBaudConfig};

use common::{ids::SourceId, mqtt::MqttConfig, zone::{MAX_AMPS, MAX_ZONES_PER_AMP}};


impl <'de>Deserialize<'de> for BaudConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de> {

        struct BaudConfigVisitor;

        impl<'de> Visitor<'de> for BaudConfigVisitor {
            type Value = BaudConfig;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "an integer baud rate of {:?} or \"auto\"", BAUD_RATES)
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                where
                    E: de::Error, {

                match v {
                    "auto" => Ok(BaudConfig::Auto),
                    v => Err(de::Error::invalid_value(de::Unexpected::Str(v), &self))
                }
            }

            fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E>
                where
                    E: de::Error, {

                Err(de::Error::invalid_value(de::Unexpected::Str("noo"), &self))
            }
        }
        
        deserializer.deserialize_any(BaudConfigVisitor)
    }
}

impl <'de>Deserialize<'de> for AdjustBaudConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de> {
        
        struct AdjustBaudConfigVisitor;

        impl<'de> Visitor<'de> for AdjustBaudConfigVisitor {
            type Value = AdjustBaudConfig;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "an integer baud rate of {:?} or \"auto\"", BAUD_RATES)
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                where
                    E: de::Error, {

                match v {
                    "off" => Ok(AdjustBaudConfig::Off),
                    "max" => Ok(AdjustBaudConfig::Max),
                    v => Err(de::Error::invalid_value(de::Unexpected::Str(v), &self))
                }
            }
        }
        
        deserializer.deserialize_any(AdjustBaudConfigVisitor)
    }
}



#[derive(Clone, Deserialize, Debug)]
pub struct CommonPortConfig {
    #[serde(with = "humantime_serde", default = "CommonPortConfig::default_read_timeout")]
    pub read_timeout: Duration
}

impl CommonPortConfig {
    fn default_read_timeout() -> Duration {Duration::from_secs(1)}
}


#[derive(Clone, Deserialize, Debug)]
pub struct SerialConfig {
    #[serde[flatten]]
    pub common: CommonPortConfig,

    pub device: String,

    #[serde(default = "SerialConfig::default_baud")]
    pub baud: BaudConfig,

    #[serde(default = "SerialConfig::default_adjust_baud")]
    pub adjust_baud: AdjustBaudConfig,

    #[serde(default = "SerialConfig::default_reset_baud")]
    pub reset_baud: bool,
}

impl SerialConfig {
    fn default_baud() -> BaudConfig { BaudConfig::Auto }

    fn default_adjust_baud() -> AdjustBaudConfig { AdjustBaudConfig::Off }
    
    fn default_reset_baud() -> bool {true}
}



#[derive(Clone, Deserialize, Debug)]
pub struct TcpConfig {
    #[serde[flatten]]
    pub common: CommonPortConfig,

    pub address: String
}


#[derive(Clone, Deserialize, Debug)]
pub struct SourceConfig {
    pub name: String,

    #[serde(default = "SourceConfig::default_enabled")]
    pub enabled: bool
}

impl SourceConfig {
    fn default_enabled() -> bool {true}
}

impl FromStr for SourceConfig {
    type Err = Void;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SourceConfig {
            name: s.to_string(),
            enabled: SourceConfig::default_enabled()
        })
    }
}


#[derive(Clone, Deserialize, Debug)]
pub struct ZoneConfig {
    pub name: String
}

impl FromStr for ZoneConfig {
    type Err = Void;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ZoneConfig {
            name: s.to_string()
        })
    }
}


/// a config ZoneId that has the special "System" zone (00)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ZoneId {
    Zone { amp: u8, zone: u8 },
    Amp(u8),
    System
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

impl FromStr for ZoneId {
    type Err = ZoneIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let i = s.parse::<u8>().map_err(|e| ZoneIdError::ParseFailure{ value: s.to_string(), source: e })?;
        ZoneId::try_from(i)
    }
}

impl Display for ZoneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id: u8 = self.into();
        
        write!(f, "{:02}", id)
    }
}


#[derive(Clone, Deserialize, Debug)]
pub struct AmpConfig {
    #[serde(with = "humantime_serde", default = "AmpConfig::default_poll_interval")]
    pub poll_interval: Duration,

    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,

    #[serde(deserialize_with = "AmpConfig::de_sources")]
    pub sources: HashMap<SourceId, SourceConfig>,

    #[serde(deserialize_with = "AmpConfig::de_zones")]
    pub zones: HashMap<ZoneId, ZoneConfig>
}

impl AmpConfig {
    fn default_poll_interval() -> Duration {Duration::from_secs(1)}

    fn de_zones<'de, D>(deserializer: D) -> Result<HashMap<ZoneId, ZoneConfig>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ValueWrapper(#[serde(deserialize_with = "de_string_or_struct")] ZoneConfig);

        let v = HashMap::<String, ValueWrapper>::deserialize(deserializer)?;
        v.into_iter().map(|(k, ValueWrapper(v))| Ok((k.parse().map_err(de::Error::custom)?, v))).collect::<>()
    }

    fn de_sources<'de, D>(deserializer: D) -> Result<HashMap<SourceId, SourceConfig>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ValueWrapper(#[serde(deserialize_with = "de_string_or_struct")] SourceConfig);

        let v = HashMap::<String, ValueWrapper>::deserialize(deserializer)?;
        v.into_iter().map(|(k, ValueWrapper(v))| { Ok((k.parse().map_err(de::Error::custom)?, v)) }).collect()
    }
}


#[derive(Clone, Deserialize, Debug)]
pub struct LoggingConfig {

}

#[derive(Clone, Deserialize, Debug)]
pub struct Config {
    pub logging: LoggingConfig,

    pub serial: Option<SerialConfig>,
    pub tcp: Option<TcpConfig>,

    pub mqtt: MqttConfig,

    pub amp: AmpConfig,
}


/// Deserialize expecting either a String or Map.
/// Strings will use the FromStr trait on T.
/// Maps will use Deserialzie on T
// from https://serde.rs/string-or-struct.html
fn de_string_or_struct<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + FromStr<Err = Void>,
    D: Deserializer<'de>,
{
    struct StringOrStruct<T>(PhantomData<fn() -> T>);

    impl<'de, T> Visitor<'de> for StringOrStruct<T>
    where
        T: Deserialize<'de> + FromStr<Err = Void>,
    {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> Result<T, E>
        where
            E: de::Error,
        {
            Ok(FromStr::from_str(value).unwrap())
        }

        fn visit_map<M>(self, map: M) -> Result<T, M::Error>
        where
            M: MapAccess<'de>,
        {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(StringOrStruct(PhantomData))
}







// impl Default for Config {
//     fn default() -> Self {
//         Self { 
//             serial: Default::default(),
//             tcp: Default::default(),
//             mqtt: Default::default(),
//             amp: Default::default()
//         }
//     }
// }

pub fn load_config(path: &PathBuf) -> Result<Config> {
    // let default_sources = (1..6).map(|i| {
    //     (
    //         SourceId::new(i),
    //         SourceConfig {
    //             name: format!("Source {}", i),
    //             enabled: true
    //         }
    //     )
    // }).collect::<HashMap<SourceId, SourceConfig>>();

    // Ok(Config {
    //     logging: LoggingConfig {  },
    //     serial: Some(SerialConfig {
    //         common: CommonPortConfig {
    //             read_timeout: Duration::from_millis(1000)
    //         },
    //         device: "/dev/ttyUSB0s".to_string(),
    //         baud: BaudConfig::Rate(9600),
    //         adjust_baud: AdjustBaudConfig::Off,
    //         reset_baud: false 
    //     }),
    //     tcp: None,
    //     mqtt: MqttConfig {
    //         url: "mqtt://localhost?client_id=mwha2mqtt".to_string()
    //     },
    //     amp: AmpConfig {
    //         poll_interval: Duration::from_millis(1000),
    //         manufacturer: None,
    //         model: None,
    //         serial: None,
    //         sources: default_sources,
    //         zones: vec![(ZoneId::zone(1, 1).unwrap(), ZoneConfig { name: "Zone 1".to_string() })].into_iter().collect(),
    //     } })
    // 

    // todo!()

    // todo: create default source IDs so there is always 6

    let f = Figment::from(Toml::file(path));

    let config: Config = match f.extract() {
        Ok(config) => config,
        Err(err) => {
            // todo: pass error to caller
            for error in err {
                eprintln!("{}", error);
            }

            panic!("Unable to load config.");
        },
    };

    //println!("config: {:?}", config);

    

    Ok(config)
}