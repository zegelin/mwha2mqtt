use std::{path::PathBuf, collections::HashMap, time::Duration, str::FromStr, marker::PhantomData, fmt};

use figment::{Figment, providers::{Format, Toml}};
use serde::{Deserialize, Deserializer, de::{Visitor, self, MapAccess}, Serialize};

use void::Void;

use anyhow::{Result, bail};

use common::{ids::SourceId, mqtt::MqttConfig, zone::{ZoneId, ranges}};


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
    pub read_timeout: Option<Duration>
}

impl CommonPortConfig {
    fn default_read_timeout() -> Option<Duration> { Some(Duration::from_secs(1)) }
}


pub const BAUD_RATES: &'static [u32] = &[9600, 19200, 38400, 57600, 115200, 230400];

#[derive(Clone, Copy, Debug)]
pub enum BaudConfig {
    Rate(u32),
    Auto,
}

#[derive(Clone, Copy, Debug)]
pub enum AdjustBaudConfig {
    Rate(u32),
    Max,
    Off
}


#[derive(Clone, Deserialize, Debug)]
pub struct SerialPortConfig {
    #[serde[flatten]]
    pub common: CommonPortConfig,

    pub device: String,

    #[serde(default = "SerialPortConfig::default_baud")]
    pub baud: BaudConfig,

    #[serde(default = "SerialPortConfig::default_adjust_baud")]
    pub adjust_baud: AdjustBaudConfig,

    #[serde(default = "SerialPortConfig::default_reset_baud")]
    pub reset_baud: bool,
}

impl SerialPortConfig {
    fn default_baud() -> BaudConfig { BaudConfig::Auto }

    fn default_adjust_baud() -> AdjustBaudConfig { AdjustBaudConfig::Off }
    
    fn default_reset_baud() -> bool { true }
}



#[derive(Clone, Deserialize, Debug)]
pub struct TcpPortConfig {
    #[serde[flatten]]
    pub common: CommonPortConfig,

    pub url: url::Url
}

#[derive(Clone, Deserialize, Debug, Default)]
pub struct SourceShairportConfig {
    pub volume_topic: Option<String>,
}


#[derive(Clone, Deserialize, Debug)]
pub struct SourceConfig {
    pub name: String,

    #[serde(default = "SourceConfig::default_enabled")]
    pub enabled: bool,

    pub shairport: SourceShairportConfig
}

impl SourceConfig {
    fn default_enabled() -> bool { true }
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            name: Default::default(),
            enabled: Self::default_enabled(),
            shairport: Default::default()
        }
    }
}

impl FromStr for SourceConfig {
    type Err = Void;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SourceConfig {
            name: s.to_string(),
            ..Default::default()
        })
    }
}

#[derive(Clone, Deserialize, Debug, Default)]
pub struct ZoneShairportConfig {
    pub max_volume: Option<u8>,
    pub volume_offset: Option<i8>
}


#[derive(Clone, Deserialize, Debug)]
pub struct ZoneConfig {
    pub name: String,

    pub shairport: ZoneShairportConfig
}

impl FromStr for ZoneConfig {
    type Err = Void;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ZoneConfig {
            name: s.to_string(),
            shairport: Default::default()
        })
    }
}


#[derive(Clone, Deserialize, Debug)]
pub struct AmpConfig {
    #[serde(with = "humantime_serde")]
    pub poll_interval: Duration,

    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,

    #[serde(deserialize_with = "AmpConfig::de_sources")]
    sources: HashMap<SourceId, SourceConfig>,

    #[serde(deserialize_with = "AmpConfig::de_zones")]
    pub zones: HashMap<ZoneId, ZoneConfig>
}

impl AmpConfig {
    /// Deserialize zone config map, permitting "string-or-struct" for each value.
    fn de_zones<'de, D>(deserializer: D) -> Result<HashMap<ZoneId, ZoneConfig>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ValueWrapper(#[serde(deserialize_with = "de_string_or_struct")] ZoneConfig);

        let v = HashMap::<String, ValueWrapper>::deserialize(deserializer)?;
        v.into_iter().map(|(k, ValueWrapper(v))| Ok((k.parse().map_err(de::Error::custom)?, v))).collect::<>()
    }

    /// Deserialize source config map, permitting "string-or-struct" for each value.
    fn de_sources<'de, D>(deserializer: D) -> Result<HashMap<SourceId, SourceConfig>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ValueWrapper(#[serde(deserialize_with = "de_string_or_struct")] SourceConfig);

        let v = HashMap::<String, ValueWrapper>::deserialize(deserializer)?;
        v.into_iter().map(|(k, ValueWrapper(v))| { Ok((k.parse().map_err(de::Error::custom)?, v)) }).collect()
    }

    pub fn sources(&self) -> HashMap<SourceId, SourceConfig> {
        let mut sources = self.sources.clone();

        // add default sources
        for i in SourceId::all() {
            if !sources.contains_key(&i) {
                sources.insert(i, SourceConfig {
                    name: format!("Source {i}"),
                    ..Default::default()
                });
            }
        };

        return sources;
    }
}


#[derive(Clone, Deserialize, Debug)]
pub struct LoggingConfig {
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum PortConfig {
    Serial(SerialPortConfig),
    Tcp(TcpPortConfig)
}


#[derive(Clone, Deserialize, Debug)]
pub struct ShairportConfig {
    #[serde(default = "ShairportConfig::default_max_zone_volume")]
    pub max_zone_volume: u8,

    #[serde(default = "ShairportConfig::default_zone_volume_offset")]
    pub zone_volume_offset: i8
}

impl ShairportConfig {
    fn default_max_zone_volume() -> u8 { *ranges::VOLUME.end() }

    fn default_zone_volume_offset() -> i8 { 0 }
}

impl Default for ShairportConfig {
    fn default() -> Self {
        Self {
            max_zone_volume: Self::default_max_zone_volume(),
            zone_volume_offset: Self::default_zone_volume_offset()
        }
    }
}


#[derive(Clone, Deserialize, Debug)]
pub struct Config {
    pub logging: LoggingConfig,

    pub port: PortConfig,

    pub mqtt: MqttConfig,

    pub amp: AmpConfig,

    pub shairport: ShairportConfig,
}


/// Deserialize, expecting either a String or Map.
/// Strings will use the FromStr trait on T.
/// Maps will use Deserialzie on T.
// from https://serde.rs/string-or-struct.html
fn de_string_or_struct<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + FromStr<Err = Void>,
    D: Deserializer<'de>,
{
    struct StringOrStruct<T>(PhantomData<T>);

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
            E: de::Error
        {
            Ok(FromStr::from_str(value).unwrap())
        }

        fn visit_map<M>(self, map: M) -> Result<T, M::Error>
        where
            M: MapAccess<'de>
        {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(StringOrStruct(PhantomData))
}




pub fn load_config(path: &PathBuf) -> Result<Config> {
    if !path.exists() {
        bail!("{}: file not found", path.to_string_lossy())
    }
    let f = Figment::from(Toml::file(path));

    Ok(f.extract()?)
}