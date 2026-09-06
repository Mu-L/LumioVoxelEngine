//! 契约无关的水管:SHA-256、Hash 链、有界缓冲。
//!
//! 这些是**本仓自有实现**,不是任何上游产物的副本。它们过去随旧合同制那份死基线的
//! 只读产物镜像一起进来;镜像随 [ADR 0014] 整棵删除后,仍要用到的这几件东西留在这里
//! 自己维护。
//!
//! 它们与体素公共语义无关——不定义字段、不定义 ID、不定义错误码;
//! 公共语义只在 [`crate::voxel_world`],唯一真值是活契约 `lumio.voxel-world.v1`。
//!
//! SHA-256 按 FIPS 180-4 实现,正确性由 `tests/plumbing.rs` 的 known-answer
//! 向量锁住;本仓不引入第三方哈希依赖(见 `Cargo.toml` 的空依赖表)。
//!
//! [ADR 0014]: ../../../.spec/decisions/0014-exit-legacy-baseline-contract-regime.md

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

/// FIPS 180-4 SHA-256。
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (data.len() as u64) * 8;
    let mut buf = data.to_vec();
    buf.push(0x80);
    while buf.len() % 64 != 56 {
        buf.push(0);
    }
    buf.extend_from_slice(&bit_len.to_be_bytes());
    for block in buf.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().enumerate().take(16) {
            *word = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut a = h;
        for (i, word) in w.iter().enumerate() {
            let s1 = a[4].rotate_right(6) ^ a[4].rotate_right(11) ^ a[4].rotate_right(25);
            let ch = (a[4] & a[5]) ^ ((!a[4]) & a[6]);
            let t1 = a[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(*word);
            let s0 = a[0].rotate_right(2) ^ a[0].rotate_right(13) ^ a[0].rotate_right(22);
            let maj = (a[0] & a[1]) ^ (a[0] & a[2]) ^ (a[1] & a[2]);
            let t2 = s0.wrapping_add(maj);
            a[7] = a[6];
            a[6] = a[5];
            a[5] = a[4];
            a[4] = a[3].wrapping_add(t1);
            a[3] = a[2];
            a[2] = a[1];
            a[1] = a[0];
            a[0] = t1.wrapping_add(t2);
        }
        for (slot, add) in h.iter_mut().zip(a.iter()) {
            *slot = slot.wrapping_add(*add);
        }
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// SHA-256 的小写十六进制表示。
pub fn sha256_hex(data: &[u8]) -> String {
    sha256(data).iter().map(|b| format!("{b:02x}")).collect()
}

/// 一个 32 字节摘要。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hash256(pub [u8; 32]);

/// Hash 链校验失败的两种形态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainBreak {
    Truncated,
    Mismatch,
}

/// 追加一条记录:`H(prev || payload)`。
pub fn hash_chain_append(prev: &Hash256, payload: &[u8]) -> Hash256 {
    let mut buf = Vec::with_capacity(32 + payload.len());
    buf.extend_from_slice(&prev.0);
    buf.extend_from_slice(payload);
    Hash256(sha256(&buf))
}

/// 校验一条记录是否确实把 `prev` 推进到 `expected`。
pub fn hash_chain_verify(
    prev: &Hash256,
    payload: &[u8],
    expected: &Hash256,
) -> Result<(), ChainBreak> {
    if hash_chain_append(prev, payload).0 == expected.0 {
        Ok(())
    } else {
        Err(ChainBreak::Mismatch)
    }
}

/// 缓冲已满:写入被拒绝,不静默截断。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferFull;

/// 声明了容量上限的字节缓冲;满载即拒绝,不增长。
pub struct BoundedBuffer {
    inner: Vec<u8>,
    cap: usize,
}

impl BoundedBuffer {
    pub fn new(cap: usize) -> Self {
        Self {
            inner: Vec::new(),
            cap,
        }
    }

    pub fn push(&mut self, byte: u8) -> Result<(), BufferFull> {
        if self.inner.len() >= self.cap {
            return Err(BufferFull);
        }
        self.inner.push(byte);
        Ok(())
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.inner
    }
}
