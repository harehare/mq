use super::Error;
use crate::eval::runtime_value::RuntimeValue;

/// Supported binary packing formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PackFormat {
    U8,
    I8,
    U16Be,
    U16Le,
    I16Be,
    I16Le,
    U32Be,
    U32Le,
    I32Be,
    I32Le,
    U64Be,
    U64Le,
    I64Be,
    I64Le,
    F32Be,
    F32Le,
    F64Be,
    F64Le,
}

impl TryFrom<&str> for PackFormat {
    type Error = Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "u8" => Ok(Self::U8),
            "i8" => Ok(Self::I8),
            "u16be" => Ok(Self::U16Be),
            "u16le" => Ok(Self::U16Le),
            "i16be" => Ok(Self::I16Be),
            "i16le" => Ok(Self::I16Le),
            "u32be" => Ok(Self::U32Be),
            "u32le" => Ok(Self::U32Le),
            "i32be" => Ok(Self::I32Be),
            "i32le" => Ok(Self::I32Le),
            "u64be" => Ok(Self::U64Be),
            "u64le" => Ok(Self::U64Le),
            "i64be" => Ok(Self::I64Be),
            "i64le" => Ok(Self::I64Le),
            "f32be" => Ok(Self::F32Be),
            "f32le" => Ok(Self::F32Le),
            "f64be" => Ok(Self::F64Be),
            "f64le" => Ok(Self::F64Le),
            _ => Err(Error::Runtime(format!(
                "unknown pack format {:?}; supported: u8, i8, u16be/le, i16be/le, u32be/le, i32be/le, u64be/le, i64be/le, f32be/le, f64be/le",
                s
            ))),
        }
    }
}

impl PackFormat {
    /// Returns the number of bytes this format requires.
    pub fn byte_size(self) -> usize {
        match self {
            Self::U8 | Self::I8 => 1,
            Self::U16Be | Self::U16Le | Self::I16Be | Self::I16Le => 2,
            Self::U32Be | Self::U32Le | Self::I32Be | Self::I32Le | Self::F32Be | Self::F32Le => 4,
            Self::U64Be | Self::U64Le | Self::I64Be | Self::I64Le | Self::F64Be | Self::F64Le => 8,
        }
    }

    /// Packs `value` into bytes according to this format.
    pub fn pack(self, value: f64) -> RuntimeValue {
        let bytes = match self {
            Self::U8 => vec![value as u8],
            Self::I8 => vec![value as i8 as u8],
            Self::U16Be => (value as u16).to_be_bytes().to_vec(),
            Self::U16Le => (value as u16).to_le_bytes().to_vec(),
            Self::I16Be => (value as i16).to_be_bytes().to_vec(),
            Self::I16Le => (value as i16).to_le_bytes().to_vec(),
            Self::U32Be => (value as u32).to_be_bytes().to_vec(),
            Self::U32Le => (value as u32).to_le_bytes().to_vec(),
            Self::I32Be => (value as i32).to_be_bytes().to_vec(),
            Self::I32Le => (value as i32).to_le_bytes().to_vec(),
            Self::U64Be => (value as u64).to_be_bytes().to_vec(),
            Self::U64Le => (value as u64).to_le_bytes().to_vec(),
            Self::I64Be => (value as i64).to_be_bytes().to_vec(),
            Self::I64Le => (value as i64).to_le_bytes().to_vec(),
            Self::F32Be => (value as f32).to_be_bytes().to_vec(),
            Self::F32Le => (value as f32).to_le_bytes().to_vec(),
            Self::F64Be => value.to_be_bytes().to_vec(),
            Self::F64Le => value.to_le_bytes().to_vec(),
        };
        RuntimeValue::Bytes(bytes)
    }

    /// Unpacks a number from `bytes` according to this format.
    pub fn unpack(self, bytes: &[u8]) -> Result<RuntimeValue, Error> {
        let required = self.byte_size();
        if bytes.len() < required {
            return Err(Error::Runtime(format!(
                "unpack: format {:?} requires {} bytes but got {}",
                self,
                required,
                bytes.len()
            )));
        }
        let value: f64 = match self {
            Self::U8 => bytes[0] as f64,
            Self::I8 => bytes[0] as i8 as f64,
            Self::U16Be => u16::from_be_bytes([bytes[0], bytes[1]]) as f64,
            Self::U16Le => u16::from_le_bytes([bytes[0], bytes[1]]) as f64,
            Self::I16Be => i16::from_be_bytes([bytes[0], bytes[1]]) as f64,
            Self::I16Le => i16::from_le_bytes([bytes[0], bytes[1]]) as f64,
            Self::U32Be => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64,
            Self::U32Le => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64,
            Self::I32Be => i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64,
            Self::I32Le => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64,
            Self::U64Be => u64::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]) as f64,
            Self::U64Le => u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]) as f64,
            Self::I64Be => i64::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]) as f64,
            Self::I64Le => i64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]) as f64,
            Self::F32Be => f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64,
            Self::F32Le => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64,
            Self::F64Be => f64::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
            Self::F64Le => f64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
        };
        Ok(RuntimeValue::Number(value.into()))
    }
}

/// Public helpers called from `builtin.rs`.
pub(super) fn pack_number(fmt: &str, value: f64) -> Result<RuntimeValue, Error> {
    let fmt: PackFormat = fmt.try_into()?;
    Ok(fmt.pack(value))
}

pub(super) fn unpack_bytes(fmt: &str, bytes: &[u8]) -> Result<RuntimeValue, Error> {
    let fmt: PackFormat = fmt.try_into()?;
    fmt.unpack(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    // --- TryFrom<&str> / try_into ---

    #[rstest]
    #[case("u8", PackFormat::U8)]
    #[case("i8", PackFormat::I8)]
    #[case("u16be", PackFormat::U16Be)]
    #[case("u16le", PackFormat::U16Le)]
    #[case("i16be", PackFormat::I16Be)]
    #[case("i16le", PackFormat::I16Le)]
    #[case("u32be", PackFormat::U32Be)]
    #[case("u32le", PackFormat::U32Le)]
    #[case("i32be", PackFormat::I32Be)]
    #[case("i32le", PackFormat::I32Le)]
    #[case("u64be", PackFormat::U64Be)]
    #[case("u64le", PackFormat::U64Le)]
    #[case("i64be", PackFormat::I64Be)]
    #[case("i64le", PackFormat::I64Le)]
    #[case("f32be", PackFormat::F32Be)]
    #[case("f32le", PackFormat::F32Le)]
    #[case("f64be", PackFormat::F64Be)]
    #[case("f64le", PackFormat::F64Le)]
    fn test_try_from_valid(#[case] input: &str, #[case] expected: PackFormat) {
        assert_eq!(PackFormat::try_from(input).unwrap(), expected);
    }

    #[rstest]
    #[case("z8")]
    #[case("u16")]
    #[case("i32")]
    #[case("")]
    #[case(" u8")]
    #[case("U32BE")]
    #[case("u32BE")]
    fn test_try_from_invalid(#[case] input: &str) {
        assert!(PackFormat::try_from(input).is_err());
    }

    #[test]
    fn test_try_from_error_message_contains_format() {
        let err = PackFormat::try_from("bad").unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("bad"), "error should mention the bad format string");
        assert!(msg.contains("u8"), "error should list supported formats");
    }

    #[test]
    fn test_try_into_syntax() {
        // Verify .try_into() ergonomics work as expected
        let fmt: Result<PackFormat, _> = "u32be".try_into();
        assert_eq!(fmt.unwrap(), PackFormat::U32Be);
    }

    // --- byte_size: all 18 variants ---

    #[rstest]
    #[case(PackFormat::U8, 1)]
    #[case(PackFormat::I8, 1)]
    #[case(PackFormat::U16Be, 2)]
    #[case(PackFormat::U16Le, 2)]
    #[case(PackFormat::I16Be, 2)]
    #[case(PackFormat::I16Le, 2)]
    #[case(PackFormat::U32Be, 4)]
    #[case(PackFormat::U32Le, 4)]
    #[case(PackFormat::I32Be, 4)]
    #[case(PackFormat::I32Le, 4)]
    #[case(PackFormat::F32Be, 4)]
    #[case(PackFormat::F32Le, 4)]
    #[case(PackFormat::U64Be, 8)]
    #[case(PackFormat::U64Le, 8)]
    #[case(PackFormat::I64Be, 8)]
    #[case(PackFormat::I64Le, 8)]
    #[case(PackFormat::F64Be, 8)]
    #[case(PackFormat::F64Le, 8)]
    fn test_byte_size(#[case] fmt: PackFormat, #[case] expected: usize) {
        assert_eq!(fmt.byte_size(), expected);
    }

    // --- pack: all formats including boundary values ---

    #[rstest]
    // u8
    #[case(PackFormat::U8, 0.0,   vec![0x00])]
    #[case(PackFormat::U8, 1.0,   vec![0x01])]
    #[case(PackFormat::U8, 255.0, vec![0xff])]
    // i8
    #[case(PackFormat::I8, 0.0,    vec![0x00])]
    #[case(PackFormat::I8, 127.0,  vec![0x7f])]
    #[case(PackFormat::I8, -1.0,   vec![0xff])]
    #[case(PackFormat::I8, -128.0, vec![0x80])]
    // u16be / u16le
    #[case(PackFormat::U16Be, 0.0,     vec![0x00, 0x00])]
    #[case(PackFormat::U16Be, 256.0,   vec![0x01, 0x00])]
    #[case(PackFormat::U16Be, 65535.0, vec![0xff, 0xff])]
    #[case(PackFormat::U16Le, 256.0,   vec![0x00, 0x01])]
    #[case(PackFormat::U16Le, 65535.0, vec![0xff, 0xff])]
    // i16be / i16le
    #[case(PackFormat::I16Be, -1.0,     vec![0xff, 0xff])]
    #[case(PackFormat::I16Be, -32768.0, vec![0x80, 0x00])]
    #[case(PackFormat::I16Be, 32767.0,  vec![0x7f, 0xff])]
    #[case(PackFormat::I16Le, -1.0,     vec![0xff, 0xff])]
    #[case(PackFormat::I16Le, -32768.0, vec![0x00, 0x80])]
    // u32be / u32le
    #[case(PackFormat::U32Be, 0.0,          vec![0x00, 0x00, 0x00, 0x00])]
    #[case(PackFormat::U32Be, 1.0,          vec![0x00, 0x00, 0x00, 0x01])]
    #[case(PackFormat::U32Be, 4294967295.0, vec![0xff, 0xff, 0xff, 0xff])]
    #[case(PackFormat::U32Le, 1.0,          vec![0x01, 0x00, 0x00, 0x00])]
    // i32be / i32le
    #[case(PackFormat::I32Be, -1.0,          vec![0xff, 0xff, 0xff, 0xff])]
    #[case(PackFormat::I32Be, -2147483648.0, vec![0x80, 0x00, 0x00, 0x00])]
    #[case(PackFormat::I32Be, 2147483647.0,  vec![0x7f, 0xff, 0xff, 0xff])]
    #[case(PackFormat::I32Le, -1.0,          vec![0xff, 0xff, 0xff, 0xff])]
    // u64be / u64le
    #[case(PackFormat::U64Be, 0.0, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    #[case(PackFormat::U64Be, 1.0, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01])]
    #[case(PackFormat::U64Le, 1.0, vec![0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    // i64be / i64le
    #[case(PackFormat::I64Be, -1.0, vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff])]
    #[case(PackFormat::I64Le, -1.0, vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff])]
    // f32be / f32le
    #[case(PackFormat::F32Be, 0.0, vec![0x00, 0x00, 0x00, 0x00])]
    #[case(PackFormat::F32Be, 1.0, vec![0x3f, 0x80, 0x00, 0x00])]
    #[case(PackFormat::F32Le, 1.0, vec![0x00, 0x00, 0x80, 0x3f])]
    // f64be / f64le
    #[case(PackFormat::F64Be, 0.0, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    #[case(PackFormat::F64Be, 1.0, vec![0x3f, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    #[case(PackFormat::F64Le, 1.0, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f])]
    fn test_pack(#[case] fmt: PackFormat, #[case] value: f64, #[case] expected: Vec<u8>) {
        assert_eq!(fmt.pack(value), RuntimeValue::Bytes(expected));
    }

    // --- unpack: all formats ---

    #[rstest]
    // u8
    #[case(PackFormat::U8, vec![0x00], 0.0)]
    #[case(PackFormat::U8, vec![0xff], 255.0)]
    // i8
    #[case(PackFormat::I8, vec![0x00], 0.0)]
    #[case(PackFormat::I8, vec![0x7f], 127.0)]
    #[case(PackFormat::I8, vec![0x80], -128.0)]
    #[case(PackFormat::I8, vec![0xff], -1.0)]
    // u16be / u16le
    #[case(PackFormat::U16Be, vec![0x01, 0x00], 256.0)]
    #[case(PackFormat::U16Be, vec![0xff, 0xff], 65535.0)]
    #[case(PackFormat::U16Le, vec![0x00, 0x01], 256.0)]
    #[case(PackFormat::U16Le, vec![0xff, 0xff], 65535.0)]
    // i16be / i16le
    #[case(PackFormat::I16Be, vec![0x7f, 0xff], 32767.0)]
    #[case(PackFormat::I16Be, vec![0x80, 0x00], -32768.0)]
    #[case(PackFormat::I16Be, vec![0xff, 0xff], -1.0)]
    #[case(PackFormat::I16Le, vec![0xff, 0x7f], 32767.0)]
    #[case(PackFormat::I16Le, vec![0x00, 0x80], -32768.0)]
    // u32be / u32le
    #[case(PackFormat::U32Be, vec![0x00, 0x00, 0x00, 0x01], 1.0)]
    #[case(PackFormat::U32Be, vec![0xff, 0xff, 0xff, 0xff], 4294967295.0)]
    #[case(PackFormat::U32Le, vec![0x01, 0x00, 0x00, 0x00], 1.0)]
    // i32be / i32le
    #[case(PackFormat::I32Be, vec![0x7f, 0xff, 0xff, 0xff], 2147483647.0)]
    #[case(PackFormat::I32Be, vec![0x80, 0x00, 0x00, 0x00], -2147483648.0)]
    #[case(PackFormat::I32Be, vec![0xff, 0xff, 0xff, 0xff], -1.0)]
    #[case(PackFormat::I32Le, vec![0x01, 0x00, 0x00, 0x00], 1.0)]
    // u64be / u64le
    #[case(PackFormat::U64Be, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01], 1.0)]
    #[case(PackFormat::U64Le, vec![0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], 1.0)]
    // i64be / i64le
    #[case(PackFormat::I64Be, vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff], -1.0)]
    #[case(PackFormat::I64Le, vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff], -1.0)]
    // f32be / f32le
    #[case(PackFormat::F32Be, vec![0x3f, 0x80, 0x00, 0x00], 1.0)]
    #[case(PackFormat::F32Le, vec![0x00, 0x00, 0x80, 0x3f], 1.0)]
    // f64be / f64le
    #[case(PackFormat::F64Be, vec![0x3f, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], 1.0)]
    #[case(PackFormat::F64Le, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f], 1.0)]
    fn test_unpack(#[case] fmt: PackFormat, #[case] bytes: Vec<u8>, #[case] expected: f64) {
        let result = fmt.unpack(&bytes).unwrap();
        match result {
            RuntimeValue::Number(n) => assert!((n.value() - expected).abs() < 1e-6),
            _ => panic!("expected Number"),
        }
    }

    // --- unpack_too_short: every format ---

    #[rstest]
    #[case(PackFormat::U8,    vec![])]
    #[case(PackFormat::I8,    vec![])]
    #[case(PackFormat::U16Be, vec![0x00])]
    #[case(PackFormat::U16Le, vec![0x00])]
    #[case(PackFormat::I16Be, vec![0x00])]
    #[case(PackFormat::I16Le, vec![0x00])]
    #[case(PackFormat::U32Be, vec![0x00, 0x00, 0x00])]
    #[case(PackFormat::U32Le, vec![0x00, 0x00, 0x00])]
    #[case(PackFormat::I32Be, vec![0x00, 0x00, 0x00])]
    #[case(PackFormat::I32Le, vec![0x00, 0x00, 0x00])]
    #[case(PackFormat::F32Be, vec![0x00, 0x00, 0x00])]
    #[case(PackFormat::F32Le, vec![0x00, 0x00, 0x00])]
    #[case(PackFormat::U64Be, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    #[case(PackFormat::U64Le, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    #[case(PackFormat::I64Be, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    #[case(PackFormat::I64Le, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    #[case(PackFormat::F64Be, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    #[case(PackFormat::F64Le, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    fn test_unpack_too_short(#[case] fmt: PackFormat, #[case] bytes: Vec<u8>) {
        let err = fmt.unpack(&bytes).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("requires") && msg.contains("bytes"),
            "error should describe the size mismatch, got: {msg}"
        );
    }

    // --- unpack accepts extra bytes (only reads what it needs) ---

    #[rstest]
    #[case(PackFormat::U8,    vec![0x01, 0xff, 0xff],             1.0)]
    #[case(PackFormat::U16Be, vec![0x00, 0x02, 0xff, 0xff],       2.0)]
    #[case(PackFormat::U32Be, vec![0x00, 0x00, 0x00, 0x03, 0xff], 3.0)]
    fn test_unpack_extra_bytes_ignored(#[case] fmt: PackFormat, #[case] bytes: Vec<u8>, #[case] expected: f64) {
        let result = fmt.unpack(&bytes).unwrap();
        assert_eq!(result, RuntimeValue::Number(expected.into()));
    }

    // --- roundtrip: all formats ---

    #[rstest]
    #[case(PackFormat::U8, 42.0)]
    #[case(PackFormat::I8,    -5.0)]
    #[case(PackFormat::U16Be, 1234.0)]
    #[case(PackFormat::U16Le, 1234.0)]
    #[case(PackFormat::I16Be, -1000.0)]
    #[case(PackFormat::I16Le, -1000.0)]
    #[case(PackFormat::U32Be, 100000.0)]
    #[case(PackFormat::U32Le, 100000.0)]
    #[case(PackFormat::I32Be, -100000.0)]
    #[case(PackFormat::I32Le, -100000.0)]
    #[case(PackFormat::U64Be, 1000000.0)]
    #[case(PackFormat::U64Le, 1000000.0)]
    #[case(PackFormat::I64Be, -1000000.0)]
    #[case(PackFormat::I64Le, -1000000.0)]
    #[case(PackFormat::F32Be, 1.5)]
    #[case(PackFormat::F32Le, 1.5)]
    #[case(PackFormat::F64Be, 1.23456789)]
    #[case(PackFormat::F64Le, 1.23456789)]
    fn test_roundtrip(#[case] fmt: PackFormat, #[case] value: f64) {
        let packed = fmt.pack(value);
        let bytes = match packed {
            RuntimeValue::Bytes(b) => b,
            _ => panic!("expected Bytes"),
        };
        assert_eq!(bytes.len(), fmt.byte_size());
        let unpacked = fmt.unpack(&bytes).unwrap();
        match unpacked {
            RuntimeValue::Number(n) => assert!((n.value() - value).abs() < 1e-5),
            _ => panic!("expected Number"),
        }
    }

    // --- pack_number / unpack_bytes wrappers ---

    #[rstest]
    #[case("u8",    255.0, vec![0xff])]
    #[case("i8",    -1.0,  vec![0xff])]
    #[case("u16be", 256.0, vec![0x01, 0x00])]
    #[case("u32be", 1.0,   vec![0x00, 0x00, 0x00, 0x01])]
    #[case("f32be", 1.0,   vec![0x3f, 0x80, 0x00, 0x00])]
    #[case("f64be", 1.0,   vec![0x3f, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    fn test_pack_number_wrapper(#[case] fmt: &str, #[case] value: f64, #[case] expected: Vec<u8>) {
        assert_eq!(pack_number(fmt, value).unwrap(), RuntimeValue::Bytes(expected));
    }

    #[rstest]
    #[case("z8")]
    #[case("u16")]
    #[case("")]
    fn test_pack_number_invalid_fmt(#[case] fmt: &str) {
        assert!(pack_number(fmt, 0.0).is_err());
    }

    #[rstest]
    #[case("u8",    &[0x2a_u8][..], 42.0)]
    #[case("i8",    &[0xff_u8][..], -1.0)]
    #[case("u16be", &[0x01_u8, 0x00][..], 256.0)]
    #[case("u32be", &[0x00_u8, 0x00, 0x00, 0x01][..], 1.0)]
    fn test_unpack_bytes_wrapper(#[case] fmt: &str, #[case] bytes: &[u8], #[case] expected: f64) {
        assert_eq!(unpack_bytes(fmt, bytes).unwrap(), RuntimeValue::Number(expected.into()));
    }

    #[rstest]
    #[case("z8")]
    #[case("u16")]
    #[case("")]
    fn test_unpack_bytes_invalid_fmt(#[case] fmt: &str) {
        assert!(unpack_bytes(fmt, &[0x00]).is_err());
    }

    #[test]
    fn test_unpack_bytes_too_short() {
        assert!(unpack_bytes("u32be", &[0x00, 0x01]).is_err());
    }
}
