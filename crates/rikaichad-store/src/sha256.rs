// SPDX-License-Identifier: AGPL-3.0-or-later
//! SHA-256 (FIPS 180-4), used to identify the source zip of an imported
//! dictionary so `reimport` can find it again on another machine.
//! Written here instead of adding a dependency for one function.

use std::io::{self, Read};

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const INITIAL: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

const BLOCK: usize = 64;

/// Hashes everything `reader` yields and returns the digest as 64
/// lowercase hex characters. Reads in 1 MiB chunks.
pub fn sha256_hex(reader: &mut impl Read) -> io::Result<String> {
    let mut state = INITIAL;
    let mut buffer = vec![0u8; 1 << 20];
    let mut pending: Vec<u8> = Vec::with_capacity(BLOCK);
    let mut total: u64 = 0;
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        let Some(chunk) = buffer.get(..n) else {
            break;
        };
        total += n as u64;
        let mut rest = chunk;
        if !pending.is_empty() {
            let take = (BLOCK - pending.len()).min(rest.len());
            let (head, tail) = rest.split_at(take);
            pending.extend_from_slice(head);
            rest = tail;
            if pending.len() == BLOCK {
                compress_blocks(&mut state, &pending);
                pending.clear();
            }
        }
        let whole = rest.len() - rest.len() % BLOCK;
        let (blocks, tail) = rest.split_at(whole);
        compress_blocks(&mut state, blocks);
        pending.extend_from_slice(tail);
    }

    // Padding: 0x80, zeros, then the bit length as a big-endian u64.
    pending.push(0x80);
    while pending.len() % BLOCK != BLOCK - 8 {
        pending.push(0);
    }
    pending.extend_from_slice(&total.wrapping_mul(8).to_be_bytes());
    compress_blocks(&mut state, &pending);

    let mut hex = String::with_capacity(64);
    for word in state {
        hex.push_str(&format!("{word:08x}"));
    }
    Ok(hex)
}

/// Processes whole 64-byte blocks; `data.len()` must be a multiple of 64.
// Indices are loop counters in 16..64 over the fixed-size array `w` (64
// words), so every index is provably in bounds.
#[allow(clippy::indexing_slicing)]
fn compress_blocks(state: &mut [u32; 8], data: &[u8]) {
    for block in data.as_chunks::<BLOCK>().0 {
        let mut w = [0u32; 64];
        let (words, _) = block.as_chunks::<4>();
        for (word, bytes) in w.iter_mut().zip(words) {
            *word = u32::from_be_bytes(*bytes);
        }
        for t in 16..64 {
            let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
            let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
            w[t] = w[t - 16]
                .wrapping_add(s0)
                .wrapping_add(w[t - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
        for (k, wt) in K.iter().zip(w.iter()) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*k)
                .wrapping_add(*wt);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *s = s.wrapping_add(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(data: &[u8]) -> String {
        sha256_hex(&mut &data[..]).unwrap()
    }

    #[test]
    fn matches_fips_180_test_vectors() {
        assert_eq!(
            hash(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hash(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hash(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            hash(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn chunk_boundaries_do_not_matter() {
        // A reader that returns 7 bytes at a time exercises the pending buffer.
        struct Dribble<'a>(&'a [u8]);
        impl Read for Dribble<'_> {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                let n = self.0.len().min(7).min(buf.len());
                buf[..n].copy_from_slice(&self.0[..n]);
                self.0 = &self.0[n..];
                Ok(n)
            }
        }
        let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        assert_eq!(sha256_hex(&mut Dribble(&data)).unwrap(), hash(&data));
    }
}
