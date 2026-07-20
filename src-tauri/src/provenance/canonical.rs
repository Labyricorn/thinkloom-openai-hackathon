use super::{CplError, CplResult};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use unicode_normalization::UnicodeNormalization;

fn utf16_cmp(left: &str, right: &str) -> Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
}

pub fn normalize_nfc(value: &Value) -> CplResult<Value> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(value.clone()),
        Value::String(text) => Ok(Value::String(text.nfc().collect())),
        Value::Array(values) => values
            .iter()
            .map(normalize_nfc)
            .collect::<CplResult<Vec<_>>>()
            .map(Value::Array),
        Value::Object(values) => {
            let mut normalized = Map::new();
            for (key, child) in values {
                let key: String = key.nfc().collect();
                if normalized.contains_key(&key) {
                    return Err(CplError::new(
                        "CPL_NFC_KEY_COLLISION",
                        format!("Multiple object keys normalize to '{key}'."),
                        false,
                    ));
                }
                normalized.insert(key, normalize_nfc(child)?);
            }
            Ok(Value::Object(normalized))
        }
    }
}

fn canonical_number(number: &Number) -> CplResult<String> {
    if let Some(value) = number.as_i64() {
        return Ok(value.to_string());
    }
    if let Some(value) = number.as_u64() {
        return Ok(value.to_string());
    }
    let value = number.as_f64().ok_or_else(|| {
        CplError::new(
            "CPL_NUMBER_INVALID",
            "The JSON number is not finite.",
            false,
        )
    })?;
    if !value.is_finite() {
        return Err(CplError::new(
            "CPL_NUMBER_INVALID",
            "NaN and infinity are prohibited.",
            false,
        ));
    }
    if value == 0.0 {
        return Ok("0".to_owned());
    }
    let mut buffer = ryu::Buffer::new();
    let rendered = buffer.format_finite(value);
    let rendered = rendered.strip_suffix(".0").unwrap_or(rendered);
    let Some(exponent_index) = rendered.find('e') else {
        return Ok(rendered.to_owned());
    };
    let (negative, unsigned) = rendered
        .strip_prefix('-')
        .map_or((false, rendered), |value| (true, value));
    let exponent_index = unsigned.find('e').unwrap_or(exponent_index);
    let mantissa = &unsigned[..exponent_index];
    let exponent: i32 = unsigned[exponent_index + 1..].parse().map_err(|error| {
        CplError::new(
            "CPL_NUMBER_INVALID",
            format!("Invalid numeric exponent: {error}"),
            false,
        )
    })?;
    let point = mantissa.find('.').unwrap_or(mantissa.len()) as i32;
    let digits = mantissa.replace('.', "");
    let decimal_position = point + exponent;
    let scientific_exponent = decimal_position - 1;
    let body = if (0..21).contains(&scientific_exponent) {
        if decimal_position as usize >= digits.len() {
            format!(
                "{}{}",
                digits,
                "0".repeat(decimal_position as usize - digits.len())
            )
        } else {
            let split = decimal_position as usize;
            format!("{}.{}", &digits[..split], &digits[split..])
        }
    } else if (-6..0).contains(&scientific_exponent) {
        format!("0.{}{}", "0".repeat((-decimal_position) as usize), digits)
    } else {
        let fraction = &digits[1..];
        let coefficient = if fraction.is_empty() {
            digits[..1].to_owned()
        } else {
            format!("{}.{}", &digits[..1], fraction)
        };
        format!(
            "{coefficient}e{}{scientific_exponent}",
            if scientific_exponent >= 0 { "+" } else { "" }
        )
    };
    Ok(if negative { format!("-{body}") } else { body })
}

fn write_canonical(value: &Value, output: &mut String) -> CplResult<()> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => output.push_str(&canonical_number(value)?),
        Value::String(value) => {
            output.push_str(&serde_json::to_string(value).map_err(|error| {
                CplError::new("CPL_CANONICALIZATION_FAILED", error.to_string(), false)
            })?)
        }
        Value::Array(values) => {
            output.push('[');
            for (index, child) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_canonical(child, output)?;
            }
            output.push(']');
        }
        Value::Object(values) => {
            output.push('{');
            let mut entries: Vec<_> = values.iter().collect();
            entries.sort_by(|(left, _), (right, _)| utf16_cmp(left, right));
            for (index, (key, child)) in entries.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(&serde_json::to_string(key).map_err(|error| {
                    CplError::new("CPL_CANONICALIZATION_FAILED", error.to_string(), false)
                })?);
                output.push(':');
                write_canonical(child, output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

pub fn canonicalize(value: &Value) -> CplResult<Vec<u8>> {
    let normalized = normalize_nfc(value)?;
    let mut output = String::new();
    write_canonical(&normalized, &mut output)?;
    Ok(output.into_bytes())
}

pub fn sha256_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}
pub fn canonical_digest(value: &Value) -> CplResult<String> {
    canonicalize(value).map(|bytes| sha256_digest(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_normative_canonicalization_vectors() {
        let cases = [
            (
                json!({"z":1,"a":2,"nested":{"beta":true,"alpha":false}}),
                "{\"a\":2,\"nested\":{\"alpha\":false,\"beta\":true},\"z\":1}",
            ),
            (
                json!({"text":"Cafe\u{301}","e\u{301}":"normalize keys and values"}),
                "{\"text\":\"Café\",\"é\":\"normalize keys and values\"}",
            ),
            (
                json!({"numbers":[333333333.3333333,1e30,4.5,0.002,1e-27]}),
                "{\"numbers\":[333333333.3333333,1e+30,4.5,0.002,1e-27]}",
            ),
            (
                json!({"numbers":[1e20,1e-6,1e21,1e-7]}),
                "{\"numbers\":[100000000000000000000,0.000001,1e+21,1e-7]}",
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(
                String::from_utf8(canonicalize(&input).unwrap()).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn rejects_nfc_key_collisions() {
        let value: Value = serde_json::from_str("{\"é\":1,\"é\":2}").unwrap();
        assert_eq!(
            normalize_nfc(&value).unwrap_err().code,
            "CPL_NFC_KEY_COLLISION"
        );
    }
}
