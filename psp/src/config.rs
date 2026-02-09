//! Configuration persistence for the PSP.
//!
//! Stores key-value pairs in a compact binary format and reads/writes
//! them via [`crate::io`]. Suitable for saving game settings or
//! application preferences to the Memory Stick.
//!
//! # Binary Format
//!
//! ```text
//! Magic: b"RCFG" (4 bytes)
//! Version: 1 (u16 LE)
//! Count: N (u16 LE)
//! Entry[N]:
//!   key_len: u8
//!   key: [u8; key_len]
//!   value_type: u8 (0=Bool, 1=I32, 2=U32, 3=F32, 4=Str, 5=Bytes)
//!   value_len: u16 LE
//!   value: [u8; value_len]
//! ```

use alloc::string::String;
use alloc::vec::Vec;

const MAGIC: &[u8; 4] = b"RCFG";
const VERSION: u16 = 1;
const MAX_FILE_SIZE: usize = 64 * 1024;

/// Error from a config operation.
pub enum ConfigError {
    /// I/O error reading or writing the file.
    Io(crate::io::IoError),
    /// The file has an invalid format or unsupported version.
    InvalidFormat,
    /// The requested key was not found.
    KeyNotFound,
    /// The serialized config exceeds the maximum size.
    TooLarge,
    /// A key exceeds 255 bytes.
    KeyTooLong,
}

impl core::fmt::Debug for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "ConfigError::Io({e:?})"),
            Self::InvalidFormat => write!(f, "ConfigError::InvalidFormat"),
            Self::KeyNotFound => write!(f, "ConfigError::KeyNotFound"),
            Self::TooLarge => write!(f, "ConfigError::TooLarge"),
            Self::KeyTooLong => write!(f, "ConfigError::KeyTooLong"),
        }
    }
}

impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "config I/O error: {e}"),
            Self::InvalidFormat => write!(f, "invalid config format"),
            Self::KeyNotFound => write!(f, "config key not found"),
            Self::TooLarge => write!(f, "config file too large"),
            Self::KeyTooLong => write!(f, "config key too long"),
        }
    }
}

impl From<crate::io::IoError> for ConfigError {
    fn from(e: crate::io::IoError) -> Self {
        Self::Io(e)
    }
}

/// A configuration value.
#[derive(Clone)]
pub enum ConfigValue {
    Bool(bool),
    I32(i32),
    U32(u32),
    F32(f32),
    Str(String),
    Bytes(Vec<u8>),
}

impl core::fmt::Debug for ConfigValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Bool(v) => write!(f, "Bool({v})"),
            Self::I32(v) => write!(f, "I32({v})"),
            Self::U32(v) => write!(f, "U32({v})"),
            Self::F32(v) => write!(f, "F32({v})"),
            Self::Str(v) => write!(f, "Str({v:?})"),
            Self::Bytes(v) => write!(f, "Bytes(len={})", v.len()),
        }
    }
}

const TYPE_BOOL: u8 = 0;
const TYPE_I32: u8 = 1;
const TYPE_U32: u8 = 2;
const TYPE_F32: u8 = 3;
const TYPE_STR: u8 = 4;
const TYPE_BYTES: u8 = 5;

/// Key-value configuration store.
pub struct Config {
    entries: Vec<(String, ConfigValue)>,
}

impl Config {
    /// Create an empty configuration.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Load a configuration from a file.
    pub fn load(path: &str) -> Result<Self, ConfigError> {
        let data = crate::io::read_to_vec(path)?;
        if data.len() > MAX_FILE_SIZE {
            return Err(ConfigError::TooLarge);
        }
        Self::deserialize(&data)
    }

    /// Save the configuration to a file.
    pub fn save(&self, path: &str) -> Result<(), ConfigError> {
        let data = self.serialize()?;
        crate::io::write_bytes(path, &data)?;
        Ok(())
    }

    /// Get a value by key.
    pub fn get(&self, key: &str) -> Option<&ConfigValue> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Set a value for a key. Overwrites if the key already exists.
    pub fn set(&mut self, key: &str, value: ConfigValue) {
        if let Some(entry) = self.entries.iter_mut().find(|(k, _)| k == key) {
            entry.1 = value;
        } else {
            self.entries.push((String::from(key), value));
        }
    }

    /// Remove a key and return its value.
    pub fn remove(&mut self, key: &str) -> Option<ConfigValue> {
        let idx = self.entries.iter().position(|(k, _)| k == key)?;
        Some(self.entries.remove(idx).1)
    }

    /// Get a value as `i32`.
    pub fn get_i32(&self, key: &str) -> Option<i32> {
        match self.get(key)? {
            ConfigValue::I32(v) => Some(*v),
            _ => None,
        }
    }

    /// Get a value as `u32`.
    pub fn get_u32(&self, key: &str) -> Option<u32> {
        match self.get(key)? {
            ConfigValue::U32(v) => Some(*v),
            _ => None,
        }
    }

    /// Get a value as `f32`.
    pub fn get_f32(&self, key: &str) -> Option<f32> {
        match self.get(key)? {
            ConfigValue::F32(v) => Some(*v),
            _ => None,
        }
    }

    /// Get a value as `bool`.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        match self.get(key)? {
            ConfigValue::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Get a value as `&str`.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            ConfigValue::Str(v) => Some(v.as_str()),
            _ => None,
        }
    }

    /// Iterate over all entries.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ConfigValue)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the config is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn serialize(&self) -> Result<Vec<u8>, ConfigError> {
        if self.entries.len() > u16::MAX as usize {
            return Err(ConfigError::TooLarge);
        }

        let mut buf = Vec::new();
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&VERSION.to_le_bytes());
        buf.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());

        for (key, value) in &self.entries {
            let key_bytes = key.as_bytes();
            if key_bytes.len() > 255 {
                return Err(ConfigError::KeyTooLong);
            }
            buf.push(key_bytes.len() as u8);
            buf.extend_from_slice(key_bytes);

            match value {
                ConfigValue::Bool(v) => {
                    buf.push(TYPE_BOOL);
                    buf.extend_from_slice(&1u16.to_le_bytes());
                    buf.push(if *v { 1 } else { 0 });
                },
                ConfigValue::I32(v) => {
                    buf.push(TYPE_I32);
                    buf.extend_from_slice(&4u16.to_le_bytes());
                    buf.extend_from_slice(&v.to_le_bytes());
                },
                ConfigValue::U32(v) => {
                    buf.push(TYPE_U32);
                    buf.extend_from_slice(&4u16.to_le_bytes());
                    buf.extend_from_slice(&v.to_le_bytes());
                },
                ConfigValue::F32(v) => {
                    buf.push(TYPE_F32);
                    buf.extend_from_slice(&4u16.to_le_bytes());
                    buf.extend_from_slice(&v.to_le_bytes());
                },
                ConfigValue::Str(v) => {
                    let bytes = v.as_bytes();
                    if bytes.len() > u16::MAX as usize {
                        return Err(ConfigError::TooLarge);
                    }
                    buf.push(TYPE_STR);
                    buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
                    buf.extend_from_slice(bytes);
                },
                ConfigValue::Bytes(v) => {
                    if v.len() > u16::MAX as usize {
                        return Err(ConfigError::TooLarge);
                    }
                    buf.push(TYPE_BYTES);
                    buf.extend_from_slice(&(v.len() as u16).to_le_bytes());
                    buf.extend_from_slice(v);
                },
            }
        }

        if buf.len() > MAX_FILE_SIZE {
            return Err(ConfigError::TooLarge);
        }
        Ok(buf)
    }

    fn deserialize(data: &[u8]) -> Result<Self, ConfigError> {
        if data.len() < 8 {
            return Err(ConfigError::InvalidFormat);
        }
        if &data[0..4] != MAGIC {
            return Err(ConfigError::InvalidFormat);
        }
        let version = u16::from_le_bytes([data[4], data[5]]);
        if version != VERSION {
            return Err(ConfigError::InvalidFormat);
        }
        let count = u16::from_le_bytes([data[6], data[7]]) as usize;

        let mut entries = Vec::with_capacity(count);
        let mut pos = 8;

        for _ in 0..count {
            if pos >= data.len() {
                return Err(ConfigError::InvalidFormat);
            }
            let key_len = data[pos] as usize;
            pos += 1;
            if pos + key_len > data.len() {
                return Err(ConfigError::InvalidFormat);
            }
            let key = core::str::from_utf8(&data[pos..pos + key_len])
                .map_err(|_| ConfigError::InvalidFormat)?;
            pos += key_len;

            if pos + 3 > data.len() {
                return Err(ConfigError::InvalidFormat);
            }
            let value_type = data[pos];
            pos += 1;
            let value_len = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
            pos += 2;

            if pos + value_len > data.len() {
                return Err(ConfigError::InvalidFormat);
            }
            let value_data = &data[pos..pos + value_len];
            pos += value_len;

            let value = match value_type {
                TYPE_BOOL => {
                    if value_len != 1 {
                        return Err(ConfigError::InvalidFormat);
                    }
                    ConfigValue::Bool(value_data[0] != 0)
                },
                TYPE_I32 => {
                    if value_len != 4 {
                        return Err(ConfigError::InvalidFormat);
                    }
                    ConfigValue::I32(i32::from_le_bytes([
                        value_data[0],
                        value_data[1],
                        value_data[2],
                        value_data[3],
                    ]))
                },
                TYPE_U32 => {
                    if value_len != 4 {
                        return Err(ConfigError::InvalidFormat);
                    }
                    ConfigValue::U32(u32::from_le_bytes([
                        value_data[0],
                        value_data[1],
                        value_data[2],
                        value_data[3],
                    ]))
                },
                TYPE_F32 => {
                    if value_len != 4 {
                        return Err(ConfigError::InvalidFormat);
                    }
                    ConfigValue::F32(f32::from_le_bytes([
                        value_data[0],
                        value_data[1],
                        value_data[2],
                        value_data[3],
                    ]))
                },
                TYPE_STR => {
                    let s =
                        core::str::from_utf8(value_data).map_err(|_| ConfigError::InvalidFormat)?;
                    ConfigValue::Str(String::from(s))
                },
                TYPE_BYTES => ConfigValue::Bytes(Vec::from(value_data)),
                _ => return Err(ConfigError::InvalidFormat),
            };

            entries.push((String::from(key), value));
        }

        Ok(Self { entries })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}
