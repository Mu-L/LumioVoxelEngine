//! `plumbing` 的 known-answer 测试:SHA-256、Hash 链、有界缓冲。
//!
//! 仓里只剩一份 SHA-256。过去有两份——生成物镜像里的那份,和 `generated_clean` 守卫
//! 为了不用被审计对象审计自己而自带的那份([ADR 0010])。镜像与守卫随 [ADR 0014]
//! 一起删除后,被审计的对象没有了,两份实现的差分守卫也随之失去对象;正确性改由
//! 下面这些公布向量单独承担。
//!
//! 期望摘要:`nist_*` 与 `million_a_*` 是 FIPS 180-4 / NIST CSRC 公布的答案;
//! 补白边界的摘要由独立参照(`openssl dgst -sha256`)产生,不是被测实现自己算的。
//!
//! [ADR 0010]: ../../../.spec/decisions/0010-generated-clean-keeps-its-own-sha256.md
//! [ADR 0014]: ../../../.spec/decisions/0014-exit-legacy-baseline-contract-regime.md

use lumio_voxel_contracts::voxel_world::SECTION_PRESENCE;
use lumio_voxel_contracts::{
    BoundedBuffer, Hash256, hash_chain_append, hash_chain_verify, sha256, sha256_hex,
};

fn assert_kat(label: &str, data: &[u8], expected: &str) {
    assert_eq!(
        sha256_hex(data),
        expected,
        "sha256_hex wrong for KAT {label} (len {})",
        data.len()
    );
}

#[test]
fn nist_empty_vector() {
    assert_kat(
        "empty",
        b"",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
}

#[test]
fn nist_abc_vector() {
    assert_kat(
        "abc",
        b"abc",
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
}

/// 448 bits: last block that still holds the length field, no extra padding block.
#[test]
fn nist_448_bit_vector() {
    assert_kat(
        "448-bit",
        b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
    );
}

/// 896 bits: forces the padding to spill into an additional block.
#[test]
fn nist_896_bit_vector() {
    assert_kat(
        "896-bit",
        b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
        "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
    );
}

/// One million 'a' — 15625 blocks, the published multi-block vector.
#[test]
fn million_a_multi_block_vector() {
    assert_kat(
        "1e6 x 'a'",
        &vec![b'a'; 1_000_000],
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
    );
}

/// Lengths where the 0x80 terminator and the 64-bit length field change block layout.
#[test]
fn padding_boundary_lengths() {
    const BOUNDARY: [(usize, &str); 10] = [
        (
            55,
            "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318",
        ),
        (
            56,
            "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a",
        ),
        (
            57,
            "f13b2d724659eb3bf47f2dd6af1accc87b81f09f59f2b75e5c0bed6589dfe8c6",
        ),
        (
            63,
            "7d3e74a05d7db15bce4ad9ec0658ea98e3f06eeecf16b4c6fff2da457ddc2f34",
        ),
        (
            64,
            "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb",
        ),
        (
            65,
            "635361c48bb9eab14198e76ea8ab7f1a41685d6ad62aa9146d301d4f17eb0ae0",
        ),
        (
            119,
            "31eba51c313a5c08226adf18d4a359cfdfd8d2e816b13f4af952f7ea6584dcfb",
        ),
        (
            120,
            "2f3d335432c70b580af0e8e1b3674a7c020d683aa5f73aaaedfdc55af904c21c",
        ),
        (
            127,
            "c57e9278af78fa3cab38667bef4ce29d783787a2f731d4e12200270f0c32320a",
        ),
        (
            128,
            "6836cf13bac400e9105071cd6af47084dfacad4e5e302c94bfed24e013afb73e",
        ),
    ];

    for (len, expected) in BOUNDARY {
        assert_kat(&format!("{len} x 'a'"), &vec![b'a'; len], expected);
    }
}

/// Hash 链与有界缓冲:追加、校验、满载拒绝。
#[test]
fn hash_chain_and_bounded_buffer() {
    let genesis = Hash256(sha256(b""));
    let next = hash_chain_append(&genesis, b"rec-1");
    assert!(hash_chain_verify(&genesis, b"rec-1", &next).is_ok());
    assert!(hash_chain_verify(&genesis, b"rec-2", &next).is_err());

    let mut buf = BoundedBuffer::new(1);
    assert!(buf.push(1).is_ok());
    assert!(buf.push(2).is_err(), "满载必须拒绝,不得静默增长");
    assert_eq!(buf.as_slice(), &[1]);
}

/// 缺块四态只从活契约取,不从任何镜像取。
#[test]
fn section_presence_comes_from_the_live_contract() {
    assert_eq!(
        SECTION_PRESENCE,
        &["Ready", "Unchanged", "Pending", "Unavailable"]
    );
}
