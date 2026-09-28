//! Bounds-checked primitives for `.mqc` payloads.
use super::MqcError;

/// Bounds the number of decoded collection entries per section. Small wire values
/// such as `None` can otherwise expand a 256 MiB file into gigabytes of VM values.
const MAX_DECODED_ITEMS: usize = 1_000_000;

pub(crate) struct Writer {
    bytes: Vec<u8>,
    remaining_items: usize,
}

impl Default for Writer {
    fn default() -> Self {
        Self {
            bytes: Vec::new(),
            remaining_items: MAX_DECODED_ITEMS,
        }
    }
}

impl Writer {
    pub(crate) fn with_remaining_items(remaining_items: usize) -> Self {
        Self {
            bytes: Vec::new(),
            remaining_items,
        }
    }

    pub(crate) fn remaining_items(&self) -> usize {
        self.remaining_items
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub(crate) fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    pub(crate) fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn f64(&mut self, value: f64) {
        self.u64(value.to_bits());
    }

    pub(crate) fn var(&mut self, mut value: u64) {
        while value >= 0x80 {
            self.bytes.push(value as u8 | 0x80);
            value >>= 7;
        }
        self.bytes.push(value as u8);
    }

    pub(crate) fn var_u16(&mut self, value: u16) {
        self.var(u64::from(value));
    }

    pub(crate) fn var_u32(&mut self, value: u32) {
        self.var(u64::from(value));
    }

    /// Zigzag, so small negatives stay short.
    pub(crate) fn var_i32(&mut self, value: i32) {
        self.var(u64::from(((value << 1) ^ (value >> 31)) as u32));
    }

    pub(crate) fn len(&mut self, len: usize) -> Result<(), MqcError> {
        let len = u32::try_from(len).map_err(|_| MqcError::Malformed("collection too large to encode".into()))?;
        if len as usize > self.remaining_items {
            return Err(MqcError::Malformed("too many collection entries to encode".into()));
        }
        self.remaining_items -= len as usize;
        self.var_u32(len);
        Ok(())
    }

    pub(crate) fn bytes(&mut self, value: &[u8]) -> Result<(), MqcError> {
        let len =
            u32::try_from(value.len()).map_err(|_| MqcError::Malformed("byte string is too large to encode".into()))?;
        self.var_u32(len);
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    pub(crate) fn str(&mut self, value: &str) -> Result<(), MqcError> {
        self.bytes(value.as_bytes())
    }

    pub(crate) fn raw(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }
}

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
    remaining_items: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            position: 0,
            remaining_items: MAX_DECODED_ITEMS,
        }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }

    pub(crate) fn finish(self, what: &str) -> Result<(), MqcError> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(MqcError::Malformed(format!("trailing bytes after {what}").into()))
        }
    }

    pub(crate) fn take(&mut self, len: usize) -> Result<&'a [u8], MqcError> {
        if len > self.remaining() {
            return Err(MqcError::Malformed("unexpected end of data".into()));
        }
        let slice = &self.bytes[self.position..self.position + len];
        self.position += len;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], MqcError> {
        let mut array = [0; N];
        array.copy_from_slice(self.take(N)?);
        Ok(array)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, MqcError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn bool(&mut self) -> Result<bool, MqcError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(MqcError::Malformed(format!("invalid boolean {other}").into())),
        }
    }

    pub(crate) fn u16(&mut self) -> Result<u16, MqcError> {
        self.array().map(u16::from_le_bytes)
    }

    pub(crate) fn u32(&mut self) -> Result<u32, MqcError> {
        self.array().map(u32::from_le_bytes)
    }

    pub(crate) fn u64(&mut self) -> Result<u64, MqcError> {
        self.array().map(u64::from_le_bytes)
    }

    pub(crate) fn f64(&mut self) -> Result<f64, MqcError> {
        self.u64().map(f64::from_bits)
    }

    pub(crate) fn var(&mut self) -> Result<u64, MqcError> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = self.u8()?;
            let part = u64::from(byte & 0x7f);
            if part << shift >> shift != part {
                break;
            }
            value |= part << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(MqcError::Malformed("varint is too long".into()))
    }

    pub(crate) fn var_u16(&mut self) -> Result<u16, MqcError> {
        u16::try_from(self.var()?).map_err(|_| MqcError::Malformed("value exceeds 16 bits".into()))
    }

    pub(crate) fn var_u32(&mut self) -> Result<u32, MqcError> {
        u32::try_from(self.var()?).map_err(|_| MqcError::Malformed("value exceeds 32 bits".into()))
    }

    pub(crate) fn var_i32(&mut self) -> Result<i32, MqcError> {
        let value = self.var_u32()?;
        Ok((value >> 1) as i32 ^ -((value & 1) as i32))
    }

    /// Rejects lengths the remaining bytes cannot hold, bounding allocations.
    pub(crate) fn len(&mut self, min_element_size: usize) -> Result<usize, MqcError> {
        let len = self.var_u32()? as usize;
        if len.saturating_mul(min_element_size.max(1)) > self.remaining() {
            return Err(MqcError::Malformed(
                "collection length exceeds the remaining data".into(),
            ));
        }
        if len > self.remaining_items {
            return Err(MqcError::Malformed("too many collection entries in a section".into()));
        }
        self.remaining_items -= len;
        Ok(len)
    }

    pub(crate) fn bytes(&mut self) -> Result<&'a [u8], MqcError> {
        // Byte strings do not expand into one Rust value per byte.
        let len = self.var_u32()? as usize;
        self.take(len)
    }

    pub(crate) fn string(&mut self) -> Result<String, MqcError> {
        let bytes = self.bytes()?;
        std::str::from_utf8(bytes)
            .map(str::to_string)
            .map_err(|_| MqcError::Malformed("string is not valid UTF-8".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn test_primitives_round_trip() {
        let mut writer = Writer::default();
        writer.u8(7);
        writer.bool(true);
        writer.u16(0xBEEF);
        writer.u32(0xDEAD_BEEF);
        writer.var_i32(-42);
        writer.var_i32(i32::MIN);
        writer.var_u16(u16::MAX);
        writer.var(u64::MAX);
        writer.u64(u64::MAX);
        writer.f64(-1.5);
        writer.str("mq").unwrap();

        let bytes = writer.into_bytes();
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.u8().unwrap(), 7);
        assert!(reader.bool().unwrap());
        assert_eq!(reader.u16().unwrap(), 0xBEEF);
        assert_eq!(reader.u32().unwrap(), 0xDEAD_BEEF);
        assert_eq!(reader.var_i32().unwrap(), -42);
        assert_eq!(reader.var_i32().unwrap(), i32::MIN);
        assert_eq!(reader.var_u16().unwrap(), u16::MAX);
        assert_eq!(reader.var().unwrap(), u64::MAX);
        assert_eq!(reader.u64().unwrap(), u64::MAX);
        assert_eq!(reader.f64().unwrap(), -1.5);
        assert_eq!(reader.string().unwrap(), "mq");
        reader.finish("test payload").unwrap();
    }

    #[test]
    fn test_reader_rejects_truncated_data() {
        let mut reader = Reader::new(&[1, 2, 3]);
        assert!(matches!(reader.u32(), Err(MqcError::Malformed(_))));
    }

    #[rstest]
    #[case::small(&[0x05], 5)]
    #[case::two_bytes(&[0x80, 0x01], 128)]
    #[case::max(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01], u64::MAX)]
    fn test_varint_decoding(#[case] bytes: &[u8], #[case] expected: u64) {
        let mut reader = Reader::new(bytes);
        assert_eq!(reader.var().unwrap(), expected);
        reader.finish("varint").unwrap();
    }

    #[rstest]
    #[case::overflow(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02])]
    #[case::too_long(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00])]
    #[case::truncated(&[0x80])]
    fn test_varint_rejects_malformed_input(#[case] bytes: &[u8]) {
        assert!(matches!(Reader::new(bytes).var(), Err(MqcError::Malformed(_))));
    }

    #[test]
    fn test_var_u16_rejects_wider_values() {
        let mut writer = Writer::default();
        writer.var(u64::from(u16::MAX) + 1);
        let bytes = writer.into_bytes();
        assert!(matches!(Reader::new(&bytes).var_u16(), Err(MqcError::Malformed(_))));
    }

    #[test]
    fn test_reader_rejects_length_beyond_payload() {
        let mut writer = Writer::default();
        writer.var_u32(1_000_000);
        let bytes = writer.into_bytes();
        assert!(matches!(Reader::new(&bytes).len(1), Err(MqcError::Malformed(_))));
    }

    #[test]
    fn test_collection_budget_covers_nested_lengths() {
        let mut reader = Reader::new(&[2, 0, 0, 1, 0]);
        reader.remaining_items = 2;
        assert_eq!(reader.len(1).unwrap(), 2);
        reader.take(2).unwrap();
        assert!(matches!(reader.len(1), Err(MqcError::Malformed(_))));

        let mut writer = Writer::with_remaining_items(2);
        writer.len(2).unwrap();
        assert!(matches!(writer.len(1), Err(MqcError::Malformed(_))));
    }

    #[test]
    fn test_byte_string_length_does_not_use_collection_budget() {
        let mut writer = Writer::with_remaining_items(0);
        writer.bytes(b"abc").unwrap();
        let bytes = writer.into_bytes();
        let mut reader = Reader::new(&bytes);
        reader.remaining_items = 0;
        assert_eq!(reader.bytes().unwrap(), b"abc");
    }

    #[test]
    fn test_reader_rejects_invalid_utf8_and_booleans() {
        let mut writer = Writer::default();
        writer.bytes(&[0xFF, 0xFE]).unwrap();
        let bytes = writer.into_bytes();
        assert!(matches!(Reader::new(&bytes).string(), Err(MqcError::Malformed(_))));
        assert!(matches!(Reader::new(&[2]).bool(), Err(MqcError::Malformed(_))));
    }
}
