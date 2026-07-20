use super::{CplError, CplResult};
use chrono::{DateTime, SecondsFormat, Utc};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

static ID_CLOCK: OnceLock<Mutex<(i64, u16)>> = OnceLock::new();

pub fn timestamp_millis_at(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)
}
pub fn timestamp_millis() -> String {
    timestamp_millis_at(Utc::now())
}

fn validate_prefix(prefix: &str) -> CplResult<()> {
    if prefix.is_empty()
        || prefix.len() > 24
        || !prefix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
    {
        return Err(CplError::new(
            "CPL_ID_PREFIX_INVALID",
            "Identifier prefixes must contain lowercase ASCII letters and underscores.",
            false,
        ));
    }
    Ok(())
}

pub fn sortable_id_at(
    prefix: &str,
    unix_millis: i64,
    sequence: u16,
    entropy: &str,
) -> CplResult<String> {
    validate_prefix(prefix)?;
    if !(0..=0x0000_ffff_ffff_ffff).contains(&unix_millis)
        || entropy.len() != 32
        || !entropy.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(CplError::new(
            "CPL_ID_COMPONENT_INVALID",
            "Sortable identifier components are invalid.",
            false,
        ));
    }
    let mut bytes = [0u8; 16];
    let timestamp = (unix_millis as u64).to_be_bytes();
    bytes[..6].copy_from_slice(&timestamp[2..]);
    bytes[6..8].copy_from_slice(&sequence.to_be_bytes());
    for (index, byte) in bytes[8..].iter_mut().enumerate() {
        *byte = u8::from_str_radix(&entropy[index * 2..index * 2 + 2], 16).map_err(|_| {
            CplError::new(
                "CPL_ID_COMPONENT_INVALID",
                "Sortable identifier entropy is invalid.",
                false,
            )
        })?;
    }
    Ok(format!("{prefix}_{}", crockford_128(bytes)))
}

fn crockford_128(bytes: [u8; 16]) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut output = String::with_capacity(26);
    let mut buffer = 0u32;
    let mut bits = 2u8;
    for byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            output.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    debug_assert_eq!(output.len(), 26);
    output
}

fn advance_clock(clock: &mut (i64, u16), observed_millis: i64) -> CplResult<(i64, u16)> {
    if observed_millis <= clock.0 {
        clock.1 = clock.1.checked_add(1).ok_or_else(|| {
            CplError::new(
                "CPL_ID_SEQUENCE_EXHAUSTED",
                "Too many identifiers in one logical millisecond.",
                true,
            )
        })?;
    } else {
        *clock = (observed_millis, 0);
    }
    Ok(*clock)
}

pub fn sortable_id(prefix: &str) -> CplResult<String> {
    let observed_millis = Utc::now().timestamp_millis();
    let mut clock = ID_CLOCK
        .get_or_init(|| Mutex::new((-1, 0)))
        .lock()
        .map_err(|_| {
            CplError::new(
                "CPL_ID_CLOCK_LOCKED",
                "Identifier clock is unavailable.",
                true,
            )
        })?;
    let (logical_millis, sequence) = advance_clock(&mut clock, observed_millis)?;
    sortable_id_at(
        prefix,
        logical_millis,
        sequence,
        &Uuid::new_v4().simple().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn identifiers_are_prefixed_and_sort_by_time_then_sequence() {
        let first = sortable_id_at("event", 1_700_000_000_000, 0, &"0".repeat(32)).unwrap();
        let second = sortable_id_at("event", 1_700_000_000_000, 1, &"0".repeat(32)).unwrap();
        let third = sortable_id_at("event", 1_700_000_000_001, 0, &"0".repeat(32)).unwrap();
        assert!(first < second && second < third);
        assert!(first.starts_with("event_"));
        assert_eq!(first.len(), "event_".len() + 26);
        assert!(first["event_".len()..]
            .bytes()
            .all(|byte| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&byte)));
    }

    #[test]
    fn clock_rollback_preserves_identifier_order() {
        let mut clock = (2_000, 3);
        assert_eq!(advance_clock(&mut clock, 1_999).unwrap(), (2_000, 4));
        assert_eq!(advance_clock(&mut clock, 2_001).unwrap(), (2_001, 0));
    }

    #[test]
    fn timestamps_are_exact_utc_milliseconds() {
        let value = Utc.with_ymd_and_hms(2026, 7, 17, 18, 42, 10).unwrap()
            + chrono::Duration::milliseconds(123);
        assert_eq!(timestamp_millis_at(value), "2026-07-17T18:42:10.123Z");
    }
}
