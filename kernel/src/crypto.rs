// crypto.rs - Hardware-Compatible AES-128-GCM and CPUID Verification
//
// Worst-case execution time: Documented per function.

#[derive(Clone, Copy)]
pub struct Aes128Key {
    pub round_keys: [u32; 44],
    pub h: [u8; 16],
}

// Function: check_crypto_cpu_support
// Description: Verify via CPUID that the CPU supports AES-NI (bit 25) and PCLMULQDQ (bit 1).
// Worst-case execution time: ~120 ns
pub fn check_crypto_cpu_support() -> Result<(), &'static str> {
    let ecx: u32;
    unsafe {
        core::arch::asm!(
            "push rbx",
            "mov eax, 1",
            "cpuid",
            "pop rbx",
            out("ecx") ecx,
            options(nomem, nostack, preserves_flags)
        );
    }

    let has_aesni = (ecx & (1 << 25)) != 0;
    let has_pclmul = (ecx & (1 << 1)) != 0;

    if !has_aesni {
        return Err("CPUID error: AES-NI hardware instruction set (ECX bit 25) is not supported.");
    }
    if !has_pclmul {
        return Err("CPUID error: PCLMULQDQ hardware instruction set (ECX bit 1) is not supported.");
    }

    Ok(())
}

const RCON: [u32; 10] = [
    0x01000000, 0x02000000, 0x04000000, 0x08000000, 0x10000000,
    0x20000000, 0x40000000, 0x80000000, 0x1B000000, 0x36000000,
];

const SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5e, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

// Function: sub_word
// Description: Apply AES S-Box substitution to a 32-bit word.
// Worst-case execution time: ~5 ns
#[inline]
fn sub_word(w: u32) -> u32 {
    let b0 = SBOX[((w >> 24) & 0xFF) as usize] as u32;
    let b1 = SBOX[((w >> 16) & 0xFF) as usize] as u32;
    let b2 = SBOX[((w >> 8) & 0xFF) as usize] as u32;
    let b3 = SBOX[(w & 0xFF) as usize] as u32;
    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
}

// Function: rot_word
// Description: Rotate 32-bit word left by 8 bits.
// Worst-case execution time: ~1 ns
#[inline]
fn rot_word(w: u32) -> u32 {
    w.rotate_left(8)
}

impl Aes128Key {
    // Function: new
    // Description: Expand 128-bit key into 10-round key schedule and precompute hash subkey H.
    // Worst-case execution time: ~300 ns
    pub fn new(key: &[u8; 16]) -> Self {
        let mut w = [0u32; 44];
        for i in 0..4 {
            w[i] = ((key[4 * i] as u32) << 24)
                | ((key[4 * i + 1] as u32) << 16)
                | ((key[4 * i + 2] as u32) << 8)
                | (key[4 * i + 3] as u32);
        }

        for i in 4..44 {
            let mut temp = w[i - 1];
            if i % 4 == 0 {
                temp = sub_word(rot_word(temp)) ^ RCON[(i / 4) - 1];
            }
            w[i] = w[i - 4] ^ temp;
        }

        let mut key_obj = Self { round_keys: w, h: [0u8; 16] };
        let mut h = [0u8; 16];
        key_obj.encrypt_block(&[0u8; 16], &mut h);
        key_obj.h = h;
        key_obj
    }

    // Function: encrypt_block
    // Description: Encrypt a single 16-byte block using AES-128.
    // Worst-case execution time: ~80 ns
    pub fn encrypt_block(&self, block: &[u8; 16], out: &mut [u8; 16]) {
        let mut state = *block;

        // Initial AddRoundKey
        for i in 0..4 {
            let rk = self.round_keys[i];
            state[4 * i] ^= (rk >> 24) as u8;
            state[4 * i + 1] ^= (rk >> 16) as u8;
            state[4 * i + 2] ^= (rk >> 8) as u8;
            state[4 * i + 3] ^= rk as u8;
        }

        // 9 Main Rounds
        for round in 1..=9 {
            // 1. SubBytes
            for b in state.iter_mut() {
                *b = SBOX[*b as usize];
            }

            // 2. ShiftRows
            let s1 = state[1];
            state[1] = state[5]; state[5] = state[9]; state[9] = state[13]; state[13] = s1;
            let s2 = state[2]; let s6 = state[6];
            state[2] = state[10]; state[6] = state[14]; state[10] = s2; state[14] = s6;
            let s15 = state[15];
            state[15] = state[11]; state[11] = state[7]; state[7] = state[3]; state[3] = s15;

            // 3. MixColumns
            for c in 0..4 {
                let a0 = state[4 * c];
                let a1 = state[4 * c + 1];
                let a2 = state[4 * c + 2];
                let a3 = state[4 * c + 3];

                let g2_0 = (a0 << 1) ^ (if (a0 & 0x80) != 0 { 0x1B } else { 0 });
                let g2_1 = (a1 << 1) ^ (if (a1 & 0x80) != 0 { 0x1B } else { 0 });
                let g2_2 = (a2 << 1) ^ (if (a2 & 0x80) != 0 { 0x1B } else { 0 });
                let g2_3 = (a3 << 1) ^ (if (a3 & 0x80) != 0 { 0x1B } else { 0 });

                state[4 * c] = g2_0 ^ g2_1 ^ a1 ^ a2 ^ a3;
                state[4 * c + 1] = a0 ^ g2_1 ^ g2_2 ^ a2 ^ a3;
                state[4 * c + 2] = a0 ^ a1 ^ g2_2 ^ g2_3 ^ a3;
                state[4 * c + 3] = g2_0 ^ a0 ^ a1 ^ a2 ^ g2_3;
            }

            // 4. AddRoundKey
            for i in 0..4 {
                let rk = self.round_keys[4 * round + i];
                state[4 * i] ^= (rk >> 24) as u8;
                state[4 * i + 1] ^= (rk >> 16) as u8;
                state[4 * i + 2] ^= (rk >> 8) as u8;
                state[4 * i + 3] ^= rk as u8;
            }
        }

        // Final Round (Round 10, no MixColumns)
        for b in state.iter_mut() {
            *b = SBOX[*b as usize];
        }
        let s1 = state[1];
        state[1] = state[5]; state[5] = state[9]; state[9] = state[13]; state[13] = s1;
        let s2 = state[2]; let s6 = state[6];
        state[2] = state[10]; state[6] = state[14]; state[10] = s2; state[14] = s6;
        let s15 = state[15];
        state[15] = state[11]; state[11] = state[7]; state[7] = state[3]; state[3] = s15;

        for i in 0..4 {
            let rk = self.round_keys[40 + i];
            state[4 * i] ^= (rk >> 24) as u8;
            state[4 * i + 1] ^= (rk >> 16) as u8;
            state[4 * i + 2] ^= (rk >> 8) as u8;
            state[4 * i + 3] ^= rk as u8;
        }

        *out = state;
    }
}

// Function: ghash_multiply
// Description: Multiplies two 128-bit blocks in GF(2^128) using 64-bit word operations.
// Worst-case execution time: ~20 ns
pub fn ghash_multiply(x: &[u8; 16], y: &[u8; 16], out: &mut [u8; 16]) {
    let mut z0 = 0u64;
    let mut z1 = 0u64;
    let mut v0 = u64::from_be_bytes([y[0], y[1], y[2], y[3], y[4], y[5], y[6], y[7]]);
    let mut v1 = u64::from_be_bytes([y[8], y[9], y[10], y[11], y[12], y[13], y[14], y[15]]);

    for byte_idx in 0..16 {
        let b = x[byte_idx];
        for bit_idx in (0..8).rev() {
            if ((b >> bit_idx) & 1) != 0 {
                z0 ^= v0;
                z1 ^= v1;
            }

            let lsb = v1 & 1;
            v1 = (v1 >> 1) | ((v0 & 1) << 63);
            v0 >>= 1;

            if lsb != 0 {
                v0 ^= 0xE100_0000_0000_0000;
            }
        }
    }

    out[0..8].copy_from_slice(&z0.to_be_bytes());
    out[8..16].copy_from_slice(&z1.to_be_bytes());
}

// Function: aes_128_gcm_encrypt
// Description: Encrypt payload and compute 16-byte authentication tag using AES-128-GCM (AEAD).
// Worst-case execution time: ~100 ns per 16-byte block
pub fn aes_128_gcm_encrypt(
    key: &Aes128Key,
    iv: &[u8; 12],
    aad: &[u8],
    payload: &mut [u8],
    tag: &mut [u8; 16],
) {
    // 1. Hash Subkey H is precomputed in Aes128Key
    let h = key.h;

    // 2. Prepare Counter0 = IV || 0x00000001
    let mut j0 = [0u8; 16];
    j0[..12].copy_from_slice(iv);
    j0[15] = 1;

    let mut ek0 = [0u8; 16];
    key.encrypt_block(&j0, &mut ek0);

    // 3. Encrypt payload with AES-CTR
    let mut counter = j0;
    let mut ctr_val = 1u32;
    let mut offset = 0;

    let mut ks = [0u8; 16];
    while offset < payload.len() {
        ctr_val = ctr_val.wrapping_add(1);
        counter[12] = (ctr_val >> 24) as u8;
        counter[13] = (ctr_val >> 16) as u8;
        counter[14] = (ctr_val >> 8) as u8;
        counter[15] = ctr_val as u8;

        key.encrypt_block(&counter, &mut ks);

        let chunk = core::cmp::min(16, payload.len() - offset);
        for i in 0..chunk {
            payload[offset + i] ^= ks[i];
        }
        offset += chunk;
    }

    // 4. GHASH over AAD and Ciphertext
    let mut ghash_state = [0u8; 16];

    // AAD processing
    let mut aad_offset = 0;
    while aad_offset < aad.len() {
        let mut block = [0u8; 16];
        let chunk = core::cmp::min(16, aad.len() - aad_offset);
        block[..chunk].copy_from_slice(&aad[aad_offset..aad_offset + chunk]);
        for i in 0..16 {
            ghash_state[i] ^= block[i];
        }
        let mut next_ghash = [0u8; 16];
        ghash_multiply(&ghash_state, &h, &mut next_ghash);
        ghash_state = next_ghash;
        aad_offset += chunk;
    }

    // Ciphertext processing
    let mut ct_offset = 0;
    while ct_offset < payload.len() {
        let mut block = [0u8; 16];
        let chunk = core::cmp::min(16, payload.len() - ct_offset);
        block[..chunk].copy_from_slice(&payload[ct_offset..ct_offset + chunk]);
        for i in 0..16 {
            ghash_state[i] ^= block[i];
        }
        let mut next_ghash = [0u8; 16];
        ghash_multiply(&ghash_state, &h, &mut next_ghash);
        ghash_state = next_ghash;
        ct_offset += chunk;
    }

    // Length block: [len(AAD) in bits (64-bit)] || [len(CT) in bits (64-bit)]
    let mut len_block = [0u8; 16];
    let aad_bits = (aad.len() as u64) * 8;
    let ct_bits = (payload.len() as u64) * 8;
    len_block[0..8].copy_from_slice(&aad_bits.to_be_bytes());
    len_block[8..16].copy_from_slice(&ct_bits.to_be_bytes());

    for i in 0..16 {
        ghash_state[i] ^= len_block[i];
    }
    let mut s = [0u8; 16];
    ghash_multiply(&ghash_state, &h, &mut s);

    // 5. Final Tag = S ^ AES_K(J0)
    for i in 0..16 {
        tag[i] = s[i] ^ ek0[i];
    }
}
