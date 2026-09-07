//! MCP token 静态加密（AES-GCM-256）。
//!
//! 设计要点：
//! - 明文 bearer token 永不裸存于 DB。存储列 `token_enc` 是 AES-GCM 密文
//!   （nonce ‖ ciphertext ‖ tag）的 hex 编码；管理员可在后台解密重查明文。
//! - `token_hash`（明文 SHA-256 hex）用于每请求 O(1) 常量查找，见 `auth.rs`。
//! - 主密钥来自环境变量 `MCP_TOKEN_ENC_KEY`（hex 编码的 32 字节，64 个 hex 字符；
//!   可用 `openssl rand -hex 32` 生成）。缺失或非法时 `mcp_enc_key()` 返回 None，
//!   调用方按「MCP 不可用」降级（拒绝签发/认证），不 panic——符合 AGENTS.md §16。
//!
//! AES-GCM-256：12 字节随机 nonce（每次加密独立生成），认证标签隐含在密文尾部。
//! nonce 复用是 AES-GCM 的唯一致命风险；这里每次 `encrypt_token` 都新取 nonce，
//! 且 nonce 与密文一同存储，解密时无需额外恢复。仅用 hex（已是直接依赖）编码，
//! 避免把 base64 由传递依赖提升为直接依赖。

use aes_gcm::aead::{Aead, Generate, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};

/// 从环境读取并解码主密钥；缺失或非法返回 None（降级，不 panic）。
///
/// 接受 hex 编码的 32 字节（64 个 hex 字符）。解码后必须正好 32 字节（AES-256）。
pub fn mcp_enc_key() -> Option<[u8; 32]> {
    let raw = std::env::var("MCP_TOKEN_ENC_KEY").ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let bytes = hex::decode(trimmed).ok()?;
    if bytes.len() == 32 {
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Some(arr)
    } else {
        None
    }
}

/// 加密明文 token，返回 `nonce‖ct‖tag` 的 hex 字符串（存入 `token_enc`）。
///
/// 密钥缺失或非法、系统随机源失败、加密失败时返回 None。
pub fn encrypt_token(plaintext: &str) -> Option<String> {
    let key_bytes = mcp_enc_key()?;
    let cipher = Aes256Gcm::new((&key_bytes).into());
    let nonce = Nonce::try_generate().ok()?; // 12 字节，每次独立
    let ct = cipher.encrypt(&nonce, plaintext.as_bytes()).ok()?;
    let mut buf = Vec::with_capacity(nonce.len() + ct.len());
    buf.extend_from_slice(nonce.as_slice());
    buf.extend_from_slice(&ct);
    Some(hex::encode(buf))
}

/// 解密 `token_enc`（`nonce‖ct‖tag` 的 hex）还原明文 token。
///
/// 失败（密钥缺失、hex 非法、密文被篡改、nonce 长度不符）统一返回 None——
/// 调用方无法区分具体原因，按「该 token 不可解密」处理（等同于失效）。
pub fn decrypt_token(enc_hex: &str) -> Option<String> {
    let key_bytes = mcp_enc_key()?;
    let buf = hex::decode(enc_hex).ok()?;
    // AES-GCM-256 nonce 固定 12 字节；短于此必然损坏。
    if buf.len() < 12 {
        return None;
    }
    let (nonce_bytes, ct) = buf.split_at(12);
    let cipher = Aes256Gcm::new((&key_bytes).into());
    let nonce = nonce_bytes.try_into().ok()?;
    let pt = cipher.decrypt(nonce, ct).ok()?;
    String::from_utf8(pt).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用：把任意 32 字节映射到临时 hex 密钥环境，跑完闭包后还原 env。
    /// 返回闭包的结果，便于 `let enc = with_key(&key, || encrypt_token(...))`。
    /// 串行化（serial_test）避免跨用例污染进程级 env。
    fn with_key<F: FnOnce() -> R, R>(key: &[u8; 32], body: F) -> R {
        let prev = std::env::var("MCP_TOKEN_ENC_KEY").ok();
        std::env::set_var("MCP_TOKEN_ENC_KEY", hex::encode(key));
        // scope guard: 还原 env
        struct Restore(Option<String>);
        impl Drop for Restore {
            fn drop(&mut self) {
                match &self.0 {
                    Some(v) => std::env::set_var("MCP_TOKEN_ENC_KEY", v),
                    None => std::env::remove_var("MCP_TOKEN_ENC_KEY"),
                }
            }
        }
        let _r = Restore(prev);
        body()
    }

    #[test]
    #[serial_test::serial]
    fn decrypt_legacy_nist_empty_plaintext() {
        // AES-GCM 0.10.3 tests/aes256gcm.rs: first NIST CAVS vector,
        // gcmEncryptExtIV256.rsp, empty plaintext and AAD.
        // https://github.com/RustCrypto/AEADs/blob/aes-gcm-v0.10.3/aes-gcm/tests/aes256gcm.rs
        let key: [u8; 32] =
            hex::decode("b52c505a37d78eda5dd34f20c22540ea1b58963cf8e5bf8ffa85f9f2492505b4")
                .unwrap()
                .try_into()
                .unwrap();
        let enc = concat!(
            "516c33929df5a3284ff463d7",
            "bdc1ac884d332457a1d2664f168c76f0"
        );
        with_key(&key, || {
            assert_eq!(decrypt_token(enc).as_deref(), Some(""));
        });
    }

    #[test]
    #[serial_test::serial]
    fn roundtrip_basic() {
        let key = [42u8; 32];
        with_key(&key, || {
            let enc = encrypt_token("ygg_secret_token_value").expect("encrypt");
            let dec = decrypt_token(&enc).expect("decrypt");
            assert_eq!(dec, "ygg_secret_token_value");
        });
    }

    #[test]
    #[serial_test::serial]
    fn roundtrip_unicode_and_long() {
        let key = [7u8; 32];
        let plaintext = "ygg_".to_string() + &"中文标题-漢字-🔑 ".repeat(500);
        with_key(&key, || {
            let enc = encrypt_token(&plaintext).expect("encrypt");
            assert_eq!(decrypt_token(&enc).expect("decrypt"), plaintext);
        });
    }

    #[test]
    #[serial_test::serial]
    fn distinct_nonces_per_encryption() {
        // 相同明文加密两次，密文必须不同（nonce 随机）；都可解回同一明文。
        let key = [9u8; 32];
        with_key(&key, || {
            let a = encrypt_token("same").expect("encrypt");
            let b = encrypt_token("same").expect("encrypt");
            assert_ne!(a, b, "nonce must differ → ciphertext differs");
            assert_eq!(decrypt_token(&a).unwrap(), "same");
            assert_eq!(decrypt_token(&b).unwrap(), "same");
        });
    }

    #[test]
    #[serial_test::serial]
    fn tampered_ciphertext_fails() {
        let key = [1u8; 32];
        with_key(&key, || {
            let mut enc = encrypt_token("payload").expect("encrypt");
            // 翻转最后一个字节（GCM tag 区域）→ 认证失败
            let mut bytes = hex::decode(&enc).unwrap();
            let last = bytes.len() - 1;
            bytes[last] ^= 0xff;
            enc = hex::encode(&bytes);
            assert_eq!(decrypt_token(&enc), None);
        });
    }

    #[test]
    #[serial_test::serial]
    fn missing_key_returns_none() {
        std::env::remove_var("MCP_TOKEN_ENC_KEY");
        assert_eq!(mcp_enc_key(), None);
        assert_eq!(encrypt_token("x"), None);
        assert_eq!(decrypt_token("deadbeef"), None);
    }

    #[test]
    #[serial_test::serial]
    fn wrong_key_cannot_decrypt() {
        let enc = {
            let key = [1u8; 32];
            with_key(&key, || encrypt_token("secret").unwrap())
        };
        // 换另一个密钥：解密必须失败（认证标签不匹配）
        with_key(&[2u8; 32], || {
            assert_eq!(decrypt_token(&enc), None);
        });
    }

    #[test]
    #[serial_test::serial]
    fn accepts_hex_encoding() {
        let key = [5u8; 32];
        std::env::set_var("MCP_TOKEN_ENC_KEY", hex::encode(key));
        assert_eq!(mcp_enc_key(), Some(key));
        std::env::remove_var("MCP_TOKEN_ENC_KEY");
    }

    #[test]
    #[serial_test::serial]
    fn rejects_odd_length_hex() {
        // 奇数长度 hex 非法 → None
        std::env::set_var("MCP_TOKEN_ENC_KEY", "abc");
        assert_eq!(mcp_enc_key(), None);
        std::env::remove_var("MCP_TOKEN_ENC_KEY");
    }

    #[test]
    #[serial_test::serial]
    fn rejects_short_key() {
        // 16 字节（32 hex 字符）非 AES-256 密钥，必须拒绝
        std::env::set_var("MCP_TOKEN_ENC_KEY", hex::encode([0u8; 16]));
        assert_eq!(mcp_enc_key(), None, "16 字节非 AES-256 密钥，必须拒绝");
        std::env::remove_var("MCP_TOKEN_ENC_KEY");
    }
}
