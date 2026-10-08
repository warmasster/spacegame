//! SipHash-2-4 (Aumasson y Bernstein, 2012): a keyed hash short enough to sign every datagram.
//! Whoever does not know the key cannot make a tag that passes, so whoever does not see a
//! session's traffic cannot put anything in it (`Channel::sign`). It is not encryption: whoever
//! sees the traffic sees what it says (and the handshake the key comes from).

/// The key of a session: two words.
pub type Key = (u64, u64);

#[inline(always)]
fn round(v: &mut [u64; 4]) {
    v[0] = v[0].wrapping_add(v[1]);
    v[1] = v[1].rotate_left(13) ^ v[0];
    v[0] = v[0].rotate_left(32);
    v[2] = v[2].wrapping_add(v[3]);
    v[3] = v[3].rotate_left(16) ^ v[2];
    v[0] = v[0].wrapping_add(v[3]);
    v[3] = v[3].rotate_left(21) ^ v[0];
    v[2] = v[2].wrapping_add(v[1]);
    v[1] = v[1].rotate_left(17) ^ v[2];
    v[2] = v[2].rotate_left(32);
}

/// The SipHash-2-4 of `data` with `key`.
pub fn hash(key: Key, data: &[u8]) -> u64 {
    let (k0, k1) = key;
    let mut v = [0x736f_6d65_7073_6575 ^ k0, 0x646f_7261_6e64_6f6d ^ k1, 0x6c79_6765_6e65_7261 ^ k0, 0x7465_6462_7974_6573 ^ k1];
    let mut chunks = data.chunks_exact(8);
    for c in &mut chunks {
        let m = u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]);
        v[3] ^= m;
        round(&mut v);
        round(&mut v);
        v[0] ^= m;
    }
    let mut last = (data.len() as u64) << 56;
    for (i, &b) in chunks.remainder().iter().enumerate() {
        last |= u64::from(b) << (8 * i);
    }
    v[3] ^= last;
    round(&mut v);
    round(&mut v);
    v[0] ^= last;
    v[2] ^= 0xff;
    for _ in 0..4 {
        round(&mut v);
    }
    v[0] ^ v[1] ^ v[2] ^ v[3]
}

/// The key of a session from what its handshake said: the server's cookie and the client's salt
/// (both know them; whoever was not on the way does not).
pub fn session_key(cookie: u64, salt: u32) -> Key {
    // (splitmix64 of the two, for the second word)
    let mut z = cookie ^ (u64::from(salt) << 17) ^ 0x9E37_79B9_7F4A_7C15;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (cookie, z ^ (z >> 31))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_siphash_2_4() {
        // (the vectors of the paper: key 00..0f, messages 00, 00 01, ...)
        let key = (0x0706_0504_0302_0100, 0x0f0e_0d0c_0b0a_0908);
        let msg: Vec<u8> = (0..64).collect();
        assert_eq!(hash(key, &msg[..0]), 0x726f_db47_dd0e_0e31);
        assert_eq!(hash(key, &msg[..1]), 0x74f8_39c5_93dc_67fd);
        assert_eq!(hash(key, &msg[..8]), 0x93f5_f579_9a93_2462);
        assert_eq!(hash(key, &msg[..15]), 0xa129_ca61_49be_45e5);
        assert_eq!(hash(key, &msg[..63]), 0x958a_324c_eb06_4572);
    }

    #[test]
    fn signing_a_datagram_costs_next_to_nothing() {
        let data = [0x5au8; 1200];
        let key = session_key(12345, 678);
        let began = std::time::Instant::now();
        let mut acc = 0u64;
        for k in 0..20_000u64 {
            acc ^= hash((key.0 ^ k, key.1), &data);
        }
        let each = began.elapsed().as_secs_f64() / 20_000.0 * 1e6;
        println!("SipHash of a whole datagram (1200 bytes): {each:.2} µs ({acc:x})");
        assert!(each < 50.0, "{each} µs");
    }
}
