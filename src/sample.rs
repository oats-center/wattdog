//! Sample model and Thornwave advertisement normalization helpers.
//!
//! A [`Sample`] is the Rust-owned representation of one passive scanner
//! advertisement. It preserves raw Thornwave values for durability while also
//! adding cautious derived fields, such as human-readable BCD versions and
//! nullable fuel-gauge values.

use std::ffi::CStr;

use chrono::{DateTime, Utc};

use crate::ffi;

/// Current Parquet schema version written for each observed advertisement.
pub const SCHEMA_VERSION: i32 = 1;
const FG_SOC_DISABLED: u8 = 0xFF;
const FG_SOC_UNKNOWN: u8 = 0xFE;
const FG_RUNTIME_DISABLED: u16 = 0xFFFF;
const FG_RUNTIME_UNKNOWN: u16 = 0xFFFE;
const FG_RUNTIME_MAX: u16 = 0xFFF0;

/// One passive Thornwave advertisement after conversion into Rust-owned data.
///
/// Field names intentionally match the Parquet schema where possible. Raw
/// fields are retained even when normalized values are unavailable so downstream
/// consumers can reprocess historical data if Thornwave semantics are clarified
/// later.
#[derive(Clone, Debug)]
pub struct Sample {
    /// Schema version associated with this row.
    pub schema_version: i32,

    /// Host observation timestamp in UTC.
    pub observed_at: DateTime<Utc>,

    /// Thornwave device serial number, used as the stable device identity.
    pub serial: u64,

    /// Raw advertised device address from the SDK.
    pub address_raw: u64,

    /// SDK-rendered display address, such as a BLE MAC address or IPv4 address.
    pub address_display: Option<String>,

    /// Derived address kind: `ble_mac`, `ipv4`, or `unknown`.
    pub address_kind: Option<String>,

    /// Advertised device name when present.
    pub name: Option<String>,

    /// SDK-rendered hardware model name when available.
    pub model: Option<String>,

    /// Raw firmware version in Thornwave BCD format.
    pub firmware_version_bcd: u16,

    /// Human-readable firmware version derived from BCD, if valid.
    pub firmware_version: Option<String>,

    /// Raw hardware revision in Thornwave BCD format.
    pub hardware_revision_bcd: u8,

    /// Human-readable hardware revision derived from BCD, if valid.
    pub hardware_revision: Option<String>,

    /// Raw device time advertised by the `PowerMon`; not assumed to be UTC.
    pub device_time_raw: u32,

    /// Raw advertised Thornwave flags.
    pub flags_raw: u32,

    /// Advertised voltage channel 1 in volts.
    pub voltage1_volts: f32,

    /// Advertised voltage channel 2 in volts.
    pub voltage2_volts: f32,

    /// Advertised current in amperes.
    pub current_amps: f32,

    /// Advertised power in watts.
    pub power_watts: f32,

    /// Raw advertised coulomb-meter value; units are intentionally unnormalized.
    pub coulomb_meter_raw: f32,

    /// Raw advertised power-meter value; units are intentionally unnormalized.
    pub power_meter_raw: f32,

    /// Advertised temperature in Celsius.
    pub temperature_celsius: f32,

    /// Whether the advertised temperature comes from an external sensor.
    pub temperature_is_external: bool,

    /// Raw Thornwave power-status enum discriminant.
    pub power_status_code: u8,

    /// SDK-rendered power-status string when available.
    pub power_status: Option<String>,

    /// Raw advertised state-of-charge byte.
    pub soc_raw: u8,

    /// Normalized state-of-charge percentage, or `None` for sentinel/invalid values.
    pub soc_percent: Option<u8>,

    /// Raw advertised runtime value.
    pub runtime_raw: u16,

    /// Normalized runtime in minutes, or `None` for sentinel/invalid values.
    pub runtime_minutes: Option<u16>,

    /// Advertised RSSI in dBm.
    pub rssi_dbm: i16,
}

impl Sample {
    /// Converts a C-compatible Thornwave advertisement into a Rust-owned sample.
    ///
    /// The conversion copies all fixed-size C strings and attaches the host UTC
    /// observation timestamp. Pointers from the FFI layer are never retained.
    #[must_use]
    pub fn from_tw_advertisement(advertisement: &ffi::TwAdvertisement) -> Self {
        let hardware_revision_bcd = advertisement.hardware_revision_bcd;
        Self {
            schema_version: SCHEMA_VERSION,
            observed_at: Utc::now(),
            serial: advertisement.serial,
            address_raw: advertisement.address_raw,
            address_display: c_char_array_to_string(&advertisement.address_display),
            address_kind: Some(address_kind(hardware_revision_bcd).to_string()),
            name: c_char_array_to_string(&advertisement.name),
            model: c_char_array_to_string(&advertisement.model),
            firmware_version_bcd: advertisement.firmware_version_bcd,
            firmware_version: format_bcd_u16(advertisement.firmware_version_bcd),
            hardware_revision_bcd,
            hardware_revision: format_bcd_u8(hardware_revision_bcd),
            device_time_raw: advertisement.device_time_raw,
            flags_raw: advertisement.flags_raw,
            voltage1_volts: advertisement.voltage1_volts,
            voltage2_volts: advertisement.voltage2_volts,
            current_amps: advertisement.current_amps,
            power_watts: advertisement.power_watts,
            coulomb_meter_raw: advertisement.coulomb_meter_raw,
            power_meter_raw: advertisement.power_meter_raw,
            temperature_celsius: advertisement.temperature_celsius,
            temperature_is_external: advertisement.temperature_is_external,
            power_status_code: advertisement.power_status_code,
            power_status: c_char_array_to_string(&advertisement.power_status_display),
            soc_raw: advertisement.soc_raw,
            soc_percent: normalize_soc(advertisement.soc_raw),
            runtime_raw: advertisement.runtime_raw,
            runtime_minutes: normalize_runtime(advertisement.runtime_raw),
            rssi_dbm: advertisement.rssi_dbm,
        }
    }

    /// Returns the host observation timestamp as Unix milliseconds.
    #[must_use]
    pub fn observed_at_unix_ms(&self) -> i64 {
        self.observed_at.timestamp_millis()
    }
}

/// Normalizes a Thornwave fuel-gauge state-of-charge byte.
///
/// Thornwave sentinel values for disabled or unknown fuel-gauge state are mapped
/// to `None`. Values above `100` are also treated as unavailable.
#[must_use]
pub fn normalize_soc(value: u8) -> Option<u8> {
    match value {
        FG_SOC_DISABLED | FG_SOC_UNKNOWN => None,
        percent if percent <= 100 => Some(percent),
        _ => None,
    }
}

/// Normalizes a Thornwave fuel-gauge runtime value.
///
/// Thornwave sentinel values for disabled or unknown runtime are mapped to
/// `None`, and values above the documented maximum are rejected.
#[must_use]
pub fn normalize_runtime(value: u16) -> Option<u16> {
    match value {
        FG_RUNTIME_DISABLED | FG_RUNTIME_UNKNOWN => None,
        minutes if minutes <= FG_RUNTIME_MAX => Some(minutes),
        _ => None,
    }
}

/// Formats a 16-bit Thornwave BCD version as `XX.YY`.
///
/// Returns `None` if any nibble contains a non-decimal BCD digit.
#[must_use]
pub fn format_bcd_u16(value: u16) -> Option<String> {
    if !is_bcd_u16(value) {
        return None;
    }
    Some(format!(
        "{}{}.{}{}",
        (value >> 12) & 0xF,
        (value >> 8) & 0xF,
        (value >> 4) & 0xF,
        value & 0xF
    ))
}

/// Formats an 8-bit Thornwave BCD revision as `X.Y`.
///
/// Returns `None` if either nibble contains a non-decimal BCD digit.
#[must_use]
pub fn format_bcd_u8(value: u8) -> Option<String> {
    if !is_bcd_u8(value) {
        return None;
    }
    Some(format!("{}.{}", (value >> 4) & 0xF, value & 0xF))
}

fn is_bcd_u16(value: u16) -> bool {
    (0..4).all(|index| ((value >> (index * 4)) & 0xF) <= 9)
}

fn is_bcd_u8(value: u8) -> bool {
    ((value >> 4) & 0xF) <= 9 && (value & 0xF) <= 9
}

fn address_kind(hardware_revision_bcd: u8) -> &'static str {
    match hardware_revision_bcd & 0xF0 {
        0x10 | 0x40 => "ipv4",
        0x20 | 0x30 => "ble_mac",
        _ => "unknown",
    }
}

fn c_char_array_to_string(buffer: &[std::os::raw::c_char]) -> Option<String> {
    if buffer.is_empty() || buffer[0] == 0 {
        return None;
    }

    let value = unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .trim()
        .to_string();

    if value.is_empty() { None } else { Some(value) }
}

#[cfg(test)]
mod tests {
    use super::{format_bcd_u8, format_bcd_u16, normalize_runtime, normalize_soc};

    #[test]
    fn formats_bcd_versions() {
        assert_eq!(format_bcd_u16(0x0120).as_deref(), Some("01.20"));
        assert_eq!(format_bcd_u8(0x34).as_deref(), Some("3.4"));
        assert_eq!(format_bcd_u16(0x0A20), None);
    }

    #[test]
    fn normalizes_fuel_gauge_sentinels() {
        assert_eq!(normalize_soc(77), Some(77));
        assert_eq!(normalize_soc(0xFE), None);
        assert_eq!(normalize_soc(0xFF), None);
        assert_eq!(normalize_runtime(123), Some(123));
        assert_eq!(normalize_runtime(0xFFFE), None);
        assert_eq!(normalize_runtime(0xFFFF), None);
    }
}
