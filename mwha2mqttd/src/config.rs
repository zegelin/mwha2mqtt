use std::{io, path::PathBuf, collections::HashMap, time::Duration, str::FromStr, marker::PhantomData, fmt, hash::Hash};

use figment::{Figment, providers::{Format, Toml}};
use serde::{Deserialize, Deserializer, de::{Visitor, self, MapAccess}};

use void::Void;

use crate::serial::{BaudConfig, BAUD_RATES, AdjustBaudConfig};

use common::ids::SourceId;
use common::zone::ZoneId;


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

                // match v.try_into::<u32>() {
                //     Ok(v) => {
                //         todo!()
                //     },
                //     Err(_) => Err(de::Error::invalid_value(de::Unexpected::Signed(v.into()), &self)),
                // }

                
            }
        }
        
        deserializer.deserialize_any(BaudConfigVisitor)
    }
}

impl <'de>Deserialize<'de> for AdjustBaudConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de> {
        todo!()
    }
}

// impl <'de>Deserialize<'de> for ZoneId {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de> {
//         todo!()
//     }
// }

// impl <'de>Deserialize<'de> for SourceId {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de> {
//         todo!()
        
//        // deserializer.deserialize_i8(visitor)
//     }
// }

#[derive(Clone, Deserialize)]
pub struct CommonPortConfig {
    #[serde(with = "humantime_serde")]
    pub read_timeout: Duration
}

#[derive(Clone, Deserialize)]
pub struct SerialConfig {
    #[serde[flatten]]
    pub common: CommonPortConfig,

    pub device: String,
    pub baud: BaudConfig,
    pub adjust_baud: AdjustBaudConfig,
    pub reset_baud: bool,
}

#[derive(Clone, Deserialize)]
pub struct TcpConfig {
    #[serde[flatten]]
    pub common: CommonPortConfig,

    pub address: String
}

#[derive(Clone, Deserialize)]
pub struct SourceConfig {
    pub name: String,
    pub enabled: bool
}

impl FromStr for SourceConfig {
    type Err = Void;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SourceConfig {
            name: s.to_string(),
            enabled: true
        })
    }
}


#[derive(Clone, Deserialize)]
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


#[derive(Clone)]
pub struct AmpConfig {
    //#[serde(with = "humantime_serde")]
    pub poll_interval: Duration,

    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,

    //#[serde(deserialize_with = "de_sources")]
    pub sources: HashMap<SourceId, SourceConfig>,

    //#[serde(deserialize_with = "de_zones")]
    pub zones: HashMap<ZoneId, ZoneConfig>
}

#[derive(Clone, Deserialize)]
pub struct MqttConfig {
    pub url: String
}


#[derive(Clone, Deserialize)]
pub struct LoggingConfig {

}

#[derive(Clone)]
pub struct Config {
    pub logging: LoggingConfig,

    pub serial: Option<SerialConfig>,
    pub tcp: Option<TcpConfig>,

    pub mqtt: MqttConfig,

    pub amp: AmpConfig,
}


// from https://serde.rs/string-or-struct.html
fn string_or_struct<'de, T, D>(deserializer: D) -> Result<T, D::Error>
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

// fn de_zones<'de, D>(deserializer: D) -> Result<HashMap<ZoneId, ZoneConfig>, D::Error>
// where
//     D: Deserializer<'de>,
// {
//     #[derive(Deserialize)]
//     struct ValueWrapper(#[serde(deserialize_with = "string_or_struct")] ZoneConfig);

//     let v = HashMap::<ZoneId, ValueWrapper>::deserialize(deserializer)?;
//     Ok(v.into_iter().map(|(k, ValueWrapper(v))| (k, v)).collect())
// }

// fn de_sources<'de, D>(deserializer: D) -> Result<HashMap<SourceId, SourceConfig>, D::Error>
// where
//     D: Deserializer<'de>,
// {
//     #[derive(Deserialize)]
//     struct ValueWrapper(#[serde(deserialize_with = "string_or_struct")] SourceConfig);

//     let v = HashMap::<SourceId, ValueWrapper>::deserialize(deserializer)?;
//     Ok(v.into_iter().map(|(k, ValueWrapper(v))| (k, v)).collect())
// }

// fn de_hashmap_string_or_struct_values<'de, D, K, T>(d: D) -> std::result::Result<HashMap<K, T>, D::Error>
// where
//     D: Deserializer<'de>,
//     K: FromStr<Err = Void> + Eq + Hash,
//     T: Deserialize<'de>,
// {
//     fn deserialize_key<'de, D, S>(d: D) -> std::result::Result<S, D::Error>
//     where
//         D: Deserializer<'de>,
//         S: FromStr<Err = Void>,
//     {
//         let value: String = Deserialize::deserialize(d)?;
//         FromStr::from_str(value).unwrap()
//     }

//     #[derive(Deserialize, Hash, Eq, PartialEq)]
//     struct KeyWrapper<S: FromStr>(#[serde(deserialize_with = "deserialize_key")] S);

//     #[derive(Deserialize)]
//     struct ValueWrapper<S>(#[serde(deserialize_with = "string_or_struct")] S);

//     let dict: HashMap<KeyWrapper<K>, ValueWrapper<T>> = Deserialize::deserialize(d)?;
//     Ok(dict.into_iter().map(|(KeyWrapper(k), ValueWrapper(v))| (k, v)).collect())
// }



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

pub fn load_config(path: &PathBuf) -> Result<Config, Box<dyn std::error::Error>> {
    let default_sources = (1..6).map(|i| {
        (
            SourceId::new(i),
            SourceConfig {
                name: format!("Source {}", i),
                enabled: true
            }
        )
    }).collect::<HashMap<SourceId, SourceConfig>>();

    Ok(Config {
        logging: LoggingConfig {  },
        serial: Some(SerialConfig {
            common: CommonPortConfig {
                read_timeout: Duration::from_millis(1000)
            },
            device: "/dev/ttyUSB0s".to_string(),
            baud: BaudConfig::Rate(9600),
            adjust_baud: AdjustBaudConfig::Off,
            reset_baud: false 
        }),
        tcp: None,
        mqtt: MqttConfig {
            url: "mqtt://localhost?client_id=mwha2mqtt".to_string()
        },
        amp: AmpConfig {
            poll_interval: Duration::from_millis(1000),
            manufacturer: None,
            model: None,
            serial: None,
            sources: default_sources,
            zones: vec![(ZoneId::zone(1, 1).unwrap(), ZoneConfig { name: "Zone 1".to_string() })].into_iter().collect(),
        } })
    // 

    // todo!()

    // let figment = Figment::from(Toml::file(path));

    // let config: Config = match figment.extract() {
    //     Ok(config) => config,
    //     Err(err) => {
    //         // todo: pass error to caller
    //         for error in err {
    //             eprintln!("{}", error);
    //         }

    //         panic!("Unable to load config.");
    //     },
    // };


    // Ok(config)
}