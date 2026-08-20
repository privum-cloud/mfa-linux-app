//! Just enough of the protobuf wire format to read and write one schema.
//!
//! `prost` would be the reflex, and it is the wrong tool here: `prost-build`
//! needs the `protoc` binary at build time, which would have to be installed on
//! every developer machine and in CI, to generate code for a single message and
//! three enums. The wire format is simpler than that dependency.
//!
//! Only the two wire types the Google Authenticator payload uses are handled:
//! varint (0) and length-delimited (2). Anything else is skipped or refused.

pub const WIRE_VARINT: u8 = 0;
pub const WIRE_LENGTH: u8 = 2;
const WIRE_FIXED64: u8 = 1;
const WIRE_FIXED32: u8 = 5;

pub struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

#[derive(Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.at >= self.data.len()
    }

    /// Read a base-128 varint.
    ///
    /// Capped at ten bytes, which is the most a u64 can occupy. Without the cap
    /// a run of continuation bytes shifts past the width of the integer, and
    /// this data arrives from a QR code someone else generated.
    pub fn read_varint(&mut self) -> Option<u64> {
        let mut value: u64 = 0;
        for byte_index in 0..10 {
            let byte = *self.data.get(self.at)?;
            self.at += 1;
            value |= u64::from(byte & 0x7f) << (7 * byte_index);
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    /// Read a field key, yielding its number and wire type.
    pub fn read_key(&mut self) -> Option<(u32, u8)> {
        let key = self.read_varint()?;
        let number = u32::try_from(key >> 3).ok()?;
        if number == 0 {
            return None;
        }
        Some((number, (key & 0x07) as u8))
    }

    /// Read a length-delimited field, refusing a length that runs off the end.
    pub fn read_bytes(&mut self) -> Option<&'a [u8]> {
        let length = usize::try_from(self.read_varint()?).ok()?;
        let end = self.at.checked_add(length)?;
        let slice = self.data.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }

    /// Step over a field this schema does not know about.
    ///
    /// Unknown fields are not an error — the format may grow, and a payload
    /// Tessera cannot fully describe is still worth importing. Groups are the
    /// exception: they carry no length, so stepping over one is guesswork.
    pub fn skip(&mut self, wire_type: u8) -> Option<()> {
        match wire_type {
            WIRE_VARINT => self.read_varint().map(|_| ()),
            WIRE_LENGTH => self.read_bytes().map(|_| ()),
            WIRE_FIXED64 => self.advance(8),
            WIRE_FIXED32 => self.advance(4),
            _ => None,
        }
    }

    fn advance(&mut self, by: usize) -> Option<()> {
        let end = self.at.checked_add(by)?;
        if end > self.data.len() {
            return None;
        }
        self.at = end;
        Some(())
    }
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn varint_field(&mut self, field: u32, value: u64) {
        self.key(field, WIRE_VARINT);
        self.varint(value);
    }

    pub fn bytes_field(&mut self, field: u32, value: &[u8]) {
        self.key(field, WIRE_LENGTH);
        self.varint(value.len() as u64);
        self.buf.extend_from_slice(value);
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }

    fn key(&mut self, field: u32, wire_type: u8) {
        self.varint((u64::from(field) << 3) | u64::from(wire_type));
    }

    fn varint(&mut self, mut value: u64) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                self.buf.push(byte);
                return;
            }
            self.buf.push(byte | 0x80);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_single_byte_varint() {
        let mut r = Reader::new(&[0x2a]);
        assert_eq!(r.read_varint(), Some(42));
        assert!(r.is_empty());
    }

    #[test]
    fn reads_a_multi_byte_varint() {
        assert_eq!(Reader::new(&[0xac, 0x02]).read_varint(), Some(300));
        assert_eq!(
            Reader::new(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01])
                .read_varint(),
            Some(u64::MAX)
        );
    }

    #[test]
    fn refuses_a_varint_that_never_ends() {
        // Eleven continuation bytes cannot describe a u64. Left unchecked this
        // is an infinite shift; the payload comes from a scanned QR code, so it
        // is not ours to trust.
        assert_eq!(Reader::new(&[0x80u8; 11]).read_varint(), None);
    }

    #[test]
    fn refuses_a_truncated_varint() {
        assert_eq!(Reader::new(&[0x80]).read_varint(), None);
    }

    #[test]
    fn reads_a_field_key_into_number_and_wire_type() {
        assert_eq!(Reader::new(&[0x0a]).read_key(), Some((1, WIRE_LENGTH)));
        assert_eq!(Reader::new(&[0x38]).read_key(), Some((7, WIRE_VARINT)));
    }

    #[test]
    fn reads_length_delimited_bytes() {
        let mut r = Reader::new(&[0x03, b'a', b'b', b'c', 0x99]);
        assert_eq!(r.read_bytes(), Some(&b"abc"[..]));
        assert!(!r.is_empty());
    }

    #[test]
    fn refuses_a_length_that_runs_past_the_end() {
        // A hostile payload can claim any length it likes.
        assert_eq!(Reader::new(&[0x40, b'a']).read_bytes(), None);
    }

    #[test]
    fn skips_fields_it_does_not_understand() {
        // Unknown fields must be stepped over, not fatal: the format may grow.
        let mut r = Reader::new(&[0x02, b'h', b'i', 0x2a]);
        assert!(r.skip(WIRE_LENGTH).is_some());
        assert_eq!(r.read_varint(), Some(42));
    }

    #[test]
    fn refuses_a_wire_type_it_cannot_step_over() {
        // Groups (3 and 4) carry no length, so skipping one is guesswork.
        assert!(Reader::new(&[0x00]).skip(3).is_none());
    }

    #[test]
    fn writes_what_it_can_read_back() {
        let mut w = Writer::new();
        w.varint_field(7, 300);
        w.bytes_field(2, b"alice@example.com");

        let encoded = w.finish();
        let mut r = Reader::new(&encoded);

        assert_eq!(r.read_key(), Some((7, WIRE_VARINT)));
        assert_eq!(r.read_varint(), Some(300));
        assert_eq!(r.read_key(), Some((2, WIRE_LENGTH)));
        assert_eq!(r.read_bytes(), Some(&b"alice@example.com"[..]));
        assert!(r.is_empty());
    }
}
