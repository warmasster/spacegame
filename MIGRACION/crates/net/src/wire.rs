//! Bytes on the wire: a writer and a reader over plain byte slices. Little-endian, no padding, no
//! allocation. The reader never panics: short or malformed input is an `Err`; the writer never
//! panics either: a write that does not fit leaves the buffer as it was and marks the writer
//! (`ok()` turns false), so a caller can `mark`, try, and `rewind`.
use glam::Vec3;

/// Why bytes could not be read (or written).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    /// The input ended before the value did.
    Short,
    /// A length is over its cap, or what was written did not fit the buffer.
    Long,
    /// The bytes are no valid value: bad UTF-8, a float that is not finite, an over-long varint, an unknown tag.
    Value,
}

pub type Wire<T> = Result<T, WireError>;

/// Writes values one after another into a slice it borrows.
pub struct Writer<'a> {
    buf: &'a mut [u8],
    pos: usize,
    ok: bool,
}

impl<'a> Writer<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Writer { buf, pos: 0, ok: true }
    }
    /// Bytes written so far.
    pub fn len(&self) -> usize {
        self.pos
    }
    pub fn is_empty(&self) -> bool {
        self.pos == 0
    }
    /// Bytes that still fit.
    pub fn left(&self) -> usize {
        self.buf.len() - self.pos
    }
    /// False once a write did not fit.
    pub fn ok(&self) -> bool {
        self.ok
    }
    /// Where we are, to come back to with `rewind`.
    pub fn mark(&self) -> usize {
        self.pos
    }
    /// Forgets what was written after `mark` (and the overflow it may have caused).
    pub fn rewind(&mut self, mark: usize) {
        self.pos = mark.min(self.pos);
        self.ok = true;
    }
    /// The length written, or `Long` if something did not fit.
    pub fn finish(self) -> Wire<usize> {
        if self.ok { Ok(self.pos) } else { Err(WireError::Long) }
    }
    pub fn bytes(&mut self, b: &[u8]) {
        if !self.ok || b.len() > self.left() {
            self.ok = false;
            return;
        }
        self.buf[self.pos..self.pos + b.len()].copy_from_slice(b);
        self.pos += b.len();
    }
    pub fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }
    pub fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }
    /// Unsigned varint (LEB128): 7 bits per byte, small numbers in one byte, at most 10 bytes.
    pub fn var(&mut self, mut v: u64) {
        let mut tmp = [0u8; 10];
        let mut n = 0;
        loop {
            let low = (v & 0x7f) as u8;
            v >>= 7;
            tmp[n] = if v == 0 { low } else { low | 0x80 };
            n += 1;
            if v == 0 {
                break;
            }
        }
        self.bytes(&tmp[..n]);
    }
    /// Signed varint (zig-zag): small magnitudes of either sign in few bytes.
    pub fn zig(&mut self, v: i64) {
        self.var(((v << 1) ^ (v >> 63)) as u64);
    }
    pub fn f32(&mut self, v: f32) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn f64(&mut self, v: f64) {
        self.bytes(&v.to_le_bytes());
    }
    /// A string: its length in bytes as a varint, then UTF-8.
    pub fn str(&mut self, s: &str) {
        let mark = self.mark();
        self.var(s.len() as u64);
        self.bytes(s.as_bytes());
        if !self.ok {
            self.pos = mark;
        }
    }
    /// Three `f32` as they are (12 bytes).
    pub fn vec3(&mut self, v: Vec3) {
        let mark = self.mark();
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
        if !self.ok {
            self.pos = mark;
        }
    }
}

/// How many bytes `var(v)` takes.
pub fn var_len(v: u64) -> usize {
    (64 - (v | 1).leading_zeros() as usize).div_ceil(7)
}

/// Reads values one after another from a slice it borrows.
#[derive(Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }
    /// Bytes not read yet.
    pub fn left(&self) -> usize {
        self.buf.len() - self.pos
    }
    pub fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }
    /// Bytes read so far.
    pub fn pos(&self) -> usize {
        self.pos
    }
    /// The next `n` bytes, borrowed from the input.
    pub fn bytes(&mut self, n: usize) -> Wire<&'a [u8]> {
        if n > self.left() {
            return Err(WireError::Short);
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    /// Everything left.
    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.buf[self.pos..];
        self.pos = self.buf.len();
        s
    }
    fn array<const N: usize>(&mut self) -> Wire<[u8; N]> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.bytes(N)?);
        Ok(a)
    }
    pub fn u8(&mut self) -> Wire<u8> {
        Ok(self.array::<1>()?[0])
    }
    pub fn u16(&mut self) -> Wire<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }
    pub fn u32(&mut self) -> Wire<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    pub fn u64(&mut self) -> Wire<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    pub fn var(&mut self) -> Wire<u64> {
        let mut v = 0u64;
        for i in 0..10 {
            let b = self.u8()?;
            // The tenth byte holds the last bit of a u64: anything more is not a number we wrote.
            if i == 9 && b > 1 {
                return Err(WireError::Value);
            }
            v |= ((b & 0x7f) as u64) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(WireError::Value)
    }
    /// A varint that must fit 32 bits.
    pub fn var32(&mut self) -> Wire<u32> {
        u32::try_from(self.var()?).map_err(|_| WireError::Value)
    }
    /// A varint that must fit 16 bits.
    pub fn var16(&mut self) -> Wire<u16> {
        u16::try_from(self.var()?).map_err(|_| WireError::Value)
    }
    pub fn zig(&mut self) -> Wire<i64> {
        let v = self.var()?;
        Ok(((v >> 1) as i64) ^ -((v & 1) as i64))
    }
    /// A finite `f32`: NaN and infinities are refused (nothing we send has them, and one would poison whatever it is mixed into).
    pub fn f32(&mut self) -> Wire<f32> {
        let v = f32::from_le_bytes(self.array()?);
        if v.is_finite() { Ok(v) } else { Err(WireError::Value) }
    }
    /// A finite `f64`.
    pub fn f64(&mut self) -> Wire<f64> {
        let v = f64::from_le_bytes(self.array()?);
        if v.is_finite() { Ok(v) } else { Err(WireError::Value) }
    }
    /// A string of at most `max` bytes, borrowed from the input.
    pub fn str(&mut self, max: usize) -> Wire<&'a str> {
        let n = self.var()?;
        if n > max as u64 {
            return Err(WireError::Long);
        }
        std::str::from_utf8(self.bytes(n as usize)?).map_err(|_| WireError::Value)
    }
    pub fn vec3(&mut self) -> Wire<Vec3> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
}
