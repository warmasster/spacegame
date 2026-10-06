//! A PNG writer with nothing behind it: the rows stored as they are (deflate blocks that do not
//! compress). For the picture `--prueba` leaves of what the launcher drew.
use std::{fs, io, path::Path};

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for b in data {
        c ^= u32::from(*b);
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

/// The bytes of a PNG of `width` by `height` from its rows of RGB.
pub fn encode(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgb.len() + height as usize);
    for row in rgb.chunks(width as usize * 3) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    let blocks = raw.chunks(65535).collect::<Vec<_>>();
    for (i, block) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let len = block.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        png.extend_from_slice(&body);
        png.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    png
}

/// Write the picture to `path`, making its folder if it is not there.
pub fn write(path: &Path, width: u32, height: u32, rgb: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, encode(width, height, rgb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checksum_is_the_standard_one() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
    }

    #[test]
    fn a_picture_has_its_signature_its_size_and_its_rows() {
        let rgb = [255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9];
        let png = encode(2, 2, &rgb);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..24], [0, 0, 0, 2, 0, 0, 0, 2]);
        assert_eq!(&png[png.len() - 12..png.len() - 4], [0, 0, 0, 0, b'I', b'E', b'N', b'D']);
        // the one stored block: its header, then each row behind its filter byte (none)
        let idat = png.windows(4).position(|w| w == b"IDAT").unwrap() + 4;
        assert_eq!(&png[idat..idat + 7], [0x78, 0x01, 1, 14, 0, !14u8, 0xff]);
        assert_eq!(&png[idat + 7..idat + 21], [0, 255, 0, 0, 0, 255, 0, 0, 0, 0, 255, 9, 9, 9]);
    }

    #[test]
    fn a_long_picture_is_cut_into_blocks_and_only_the_last_says_so() {
        let (w, h) = (300u32, 100u32);
        let png = encode(w, h, &vec![7; (w * h * 3) as usize]);
        let idat = png.windows(4).position(|w| w == b"IDAT").unwrap() + 4;
        let raw = (w * 3 + 1) * h;
        assert_eq!(png[idat + 2], 0, "not the last block");
        assert_eq!(u16::from_le_bytes([png[idat + 3], png[idat + 4]]), 65535);
        let second = idat + 2 + 5 + 65535;
        assert_eq!(png[second], 1, "the last block");
        assert_eq!(u32::from(u16::from_le_bytes([png[second + 1], png[second + 2]])), raw - 65535);
    }
}
