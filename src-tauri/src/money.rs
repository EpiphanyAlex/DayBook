use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{AppError, AppResult};

pub const MAX_ABS_VALUE: i64 = 1_000_000_000_000_000;
pub const RATE_SCALE: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecimalI64(pub i64);

impl Serialize for DecimalI64 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ensure_range(self.0).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for DecimalI64 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        if raw.is_empty()
            || raw.starts_with('+')
            || (raw.starts_with('0') && raw.len() > 1)
            || (raw.starts_with("-0") && raw.len() > 2)
            || !raw
                .strip_prefix('-')
                .unwrap_or(&raw)
                .chars()
                .all(|character| character.is_ascii_digit())
        {
            return Err(D::Error::custom("金额必须是规范十进制整数字符串"));
        }
        let value = raw.parse::<i64>().map_err(D::Error::custom)?;
        ensure_range(value).map_err(D::Error::custom)?;
        Ok(Self(value))
    }
}

pub fn ensure_range(value: i64) -> AppResult<i64> {
    if value.unsigned_abs() > MAX_ABS_VALUE as u64 {
        return Err(AppError::new(
            "data.amount_out_of_range",
            "金额或汇率超出支持范围",
        ));
    }
    Ok(value)
}

pub fn currency_exponent(code: &str) -> AppResult<u32> {
    let exponent = match code {
        "BIF" | "CLP" | "DJF" | "GNF" | "ISK" | "JPY" | "KMF" | "KRW" | "PYG" | "RWF" | "UGX"
        | "UYI" | "VND" | "VUV" | "XAF" | "XOF" | "XPF" => 0,
        "BHD" | "IQD" | "JOD" | "KWD" | "LYD" | "OMR" | "TND" => 3,
        "CLF" | "UYW" => 4,
        "AED" | "AFN" | "ALL" | "AMD" | "ANG" | "AOA" | "ARS" | "AUD" | "AWG" | "AZN" | "BAM"
        | "BBD" | "BDT" | "BGN" | "BMD" | "BND" | "BOB" | "BOV" | "BRL" | "BSD" | "BTN" | "BWP"
        | "BYN" | "BZD" | "CAD" | "CDF" | "CHE" | "CHF" | "CHW" | "CNY" | "COP" | "COU" | "CRC"
        | "CUP" | "CVE" | "CZK" | "DKK" | "DOP" | "DZD" | "EGP" | "ERN" | "ETB" | "EUR" | "FJD"
        | "FKP" | "GBP" | "GEL" | "GHS" | "GIP" | "GMD" | "GTQ" | "GYD" | "HKD" | "HNL" | "HTG"
        | "HUF" | "IDR" | "ILS" | "INR" | "IRR" | "JMD" | "KES" | "KGS" | "KHR" | "KPW" | "KYD"
        | "KZT" | "LAK" | "LBP" | "LKR" | "LRD" | "LSL" | "MAD" | "MDL" | "MGA" | "MKD" | "MMK"
        | "MNT" | "MOP" | "MRU" | "MUR" | "MVR" | "MWK" | "MXN" | "MXV" | "MYR" | "MZN" | "NAD"
        | "NGN" | "NIO" | "NOK" | "NPR" | "NZD" | "PAB" | "PEN" | "PGK" | "PHP" | "PKR" | "PLN"
        | "QAR" | "RON" | "RSD" | "RUB" | "SAR" | "SBD" | "SCR" | "SDG" | "SEK" | "SGD" | "SHP"
        | "SLE" | "SOS" | "SRD" | "SSP" | "STN" | "SVC" | "SYP" | "SZL" | "THB" | "TJS" | "TMT"
        | "TOP" | "TRY" | "TTD" | "TWD" | "TZS" | "UAH" | "USD" | "USN" | "UYU" | "UZS" | "VED"
        | "VES" | "WST" | "XCD" | "YER" | "ZAR" | "ZMW" | "ZWG" => 2,
        _ => {
            return Err(AppError::new(
                "data.unsupported_currency",
                format!("不支持的 ISO 4217 币种：{code}"),
            ))
        }
    };
    Ok(exponent)
}

pub fn convert_minor(
    amount_minor: i64,
    currency: &str,
    base_currency: &str,
    rate_ppm: i64,
) -> AppResult<i64> {
    ensure_range(amount_minor)?;
    ensure_range(rate_ppm)?;
    if rate_ppm <= 0 {
        return Err(AppError::invalid_argument("汇率必须大于零"));
    }

    let source_power = 10_i128.pow(currency_exponent(currency)?);
    let base_power = 10_i128.pow(currency_exponent(base_currency)?);
    let numerator = i128::from(amount_minor) * i128::from(rate_ppm) * base_power;
    let denominator = i128::from(RATE_SCALE) * source_power;
    let rounded = round_half_even(numerator, denominator);
    let result = i64::try_from(rounded)
        .map_err(|_| AppError::new("data.amount_out_of_range", "换算后的金额超出支持范围"))?;
    ensure_range(result)
}

pub fn validate_triple(
    amount_minor: i64,
    currency: &str,
    base_amount_minor: i64,
    base_currency: &str,
    rate_ppm: i64,
) -> AppResult<()> {
    let expected = convert_minor(amount_minor, currency, base_currency, rate_ppm)?;
    ensure_range(base_amount_minor)?;
    if expected != base_amount_minor {
        return Err(AppError::new(
            "data.money_inconsistent",
            "金额、币种与汇率三元组不自洽",
        ));
    }
    Ok(())
}

fn round_half_even(numerator: i128, denominator: i128) -> i128 {
    debug_assert!(denominator > 0);
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let twice_remainder = remainder.unsigned_abs() * 2;
    let denominator_abs = denominator as u128;
    let should_round = twice_remainder > denominator_abs
        || (twice_remainder == denominator_abs && quotient % 2 != 0);
    if should_round {
        quotient + numerator.signum()
    } else {
        quotient
    }
}

#[cfg(test)]
mod foundation {
    use super::*;

    #[test]
    fn currency_exponent_is_not_hardcoded_two() {
        assert_eq!(currency_exponent("JPY").unwrap(), 0);
        assert_eq!(currency_exponent("KWD").unwrap(), 3);
        assert_eq!(currency_exponent("AUD").unwrap(), 2);
        assert_eq!(
            currency_exponent("ZZZ").unwrap_err().code,
            "data.unsupported_currency"
        );
    }

    #[test]
    fn convert_across_exponents() {
        assert_eq!(convert_minor(100, "AUD", "JPY", 100_000_000).unwrap(), 100);
        assert_eq!(convert_minor(100, "JPY", "AUD", 10_000).unwrap(), 100);
    }

    #[test]
    fn half_even_rounds_ties_to_even_for_both_signs() {
        assert_eq!(round_half_even(5, 2), 2);
        assert_eq!(round_half_even(7, 2), 4);
        assert_eq!(round_half_even(-5, 2), -2);
        assert_eq!(round_half_even(-7, 2), -4);
    }

    #[test]
    fn money_roundtrip_is_integer() {
        let encoded = serde_json::to_string(&DecimalI64(168)).unwrap();
        assert_eq!(encoded, "\"168\"");
        assert_eq!(serde_json::from_str::<DecimalI64>(&encoded).unwrap().0, 168);
    }

    #[test]
    fn money_crosses_ipc_as_string() {
        for value in [MAX_ABS_VALUE, -MAX_ABS_VALUE, RATE_SCALE] {
            let encoded = serde_json::to_value(DecimalI64(value)).unwrap();
            assert_eq!(encoded, serde_json::Value::String(value.to_string()));
            assert_eq!(
                serde_json::from_value::<DecimalI64>(encoded).unwrap().0,
                value
            );
        }
        assert!(serde_json::to_value(DecimalI64(i64::MAX)).is_err());
    }

    #[test]
    fn unknown_currency_rejected() {
        assert_eq!(
            convert_minor(100, "NOPE", "AUD", RATE_SCALE)
                .unwrap_err()
                .code,
            "data.unsupported_currency"
        );
    }

    #[test]
    fn amount_out_of_range_rejected() {
        assert_eq!(
            ensure_range(MAX_ABS_VALUE + 1).unwrap_err().code,
            "data.amount_out_of_range"
        );
    }

    #[test]
    fn money_inconsistent_rejected() {
        assert_eq!(
            validate_triple(100, "AUD", 99, "AUD", RATE_SCALE)
                .unwrap_err()
                .code,
            "data.money_inconsistent"
        );
    }
}
