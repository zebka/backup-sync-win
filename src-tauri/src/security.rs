//! امنیت رمز ورود برنامه.
//!
//! رمز هرگز به صورت متن ساده ذخیره نمی‌شود: تنها هش PBKDF2-HMAC-SHA256
//! همراه salt تصادفی و تعداد تکرار ذخیره می‌شود. قالب ذخیره:
//! `pbkdf2-sha256$<iterations>$<salt-hex>$<hash-hex>`.

use sha2::{Digest, Sha256};

const ITERATIONS: u32 = 100_000;
const MAX_SECRET_LEN: usize = 200;

/// HMAC-SHA256 بدون وابستگی به crate اضافی (فقط sha2).
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut key_block = [0u8; 64];
    if key.len() > 64 {
        let hashed = Sha256::digest(key);
        key_block[..32].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut inner = vec![0u8; 64 + data.len()];
    let mut outer = [0u8; 64 + 32];
    for i in 0..64 {
        inner[i] = key_block[i] ^ 0x36;
        outer[i] = key_block[i] ^ 0x5c;
    }
    inner[64..].copy_from_slice(data);
    let inner_hash = Sha256::digest(&inner);
    outer[64..].copy_from_slice(&inner_hash);
    let out = Sha256::digest(&outer);

    let mut result = [0u8; 32];
    result.copy_from_slice(&out);
    result
}

/// PBKDF2-HMAC-SHA256 با طول خروجی ۳۲ بایت.
fn derive(secret: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut first = Vec::with_capacity(salt.len() + 4);
    first.extend_from_slice(salt);
    first.extend_from_slice(&1u32.to_be_bytes());

    let mut u = hmac_sha256(secret.as_bytes(), &first);
    let mut result = u;
    for _ in 2..=iterations {
        u = hmac_sha256(secret.as_bytes(), &u);
        for (acc, byte) in result.iter_mut().zip(u.iter()) {
            *acc ^= *byte;
        }
    }
    result
}

fn validate_secret(secret: &str) -> Result<(), String> {
    if secret.is_empty() || secret.len() > MAX_SECRET_LEN {
        return Err(crate::settings::tr(
            "رمز باید بین ۱ تا ۲۰۰ کاراکتر باشد.",
            "The password must be 1 to 200 characters.",
        ));
    }
    Ok(())
}

/// ساخت هش ذخیره‌شدنی برای رمز یا کد بازیابی.
pub fn hash_secret(secret: &str) -> Result<String, String> {
    validate_secret(secret)?;
    use rand::Rng;
    let mut salt = [0u8; 16];
    rand::rng().fill_bytes(&mut salt);
    Ok(format!(
        "pbkdf2-sha256${}${}${}",
        ITERATIONS,
        hex::encode(salt),
        hex::encode(derive(secret, &salt, ITERATIONS))
    ))
}

/// بررسی رمز واردشده در برابر هش ذخیره‌شده.
pub fn verify_secret(secret: &str, encoded: &str) -> bool {
    if secret.is_empty() || secret.len() > MAX_SECRET_LEN {
        return false;
    }
    let parts: Vec<&str> = encoded.split('$').collect();
    if parts.len() != 4 || parts[0] != "pbkdf2-sha256" {
        return false;
    }
    let Ok(iterations) = parts[1].parse::<u32>() else {
        return false;
    };
    if !(1..=1_000_000).contains(&iterations) {
        return false;
    }
    let Ok(salt) = hex::decode(parts[2]) else {
        return false;
    };
    let Ok(expected) = hex::decode(parts[3]) else {
        return false;
    };
    if expected.len() != 32 {
        return false;
    }
    let actual = derive(secret, &salt, iterations);
    // مقایسهٔ ثابت-زمان تا حد امکان (جلوگیری از timing attack ساده)
    let mut diff = 0u8;
    for (a, b) in actual.iter().zip(expected.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// کد بازیابی تک‑دفعی: ۸ بایت تصادفی به صورت XXXX-XXXX-XXXX-XXXX.
pub fn generate_recovery_code() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 8];
    rand::rng().fill_bytes(&mut bytes);
    let raw = hex::encode(bytes).to_uppercase();
    format!(
        "{}-{}-{}-{}",
        &raw[0..4],
        &raw[4..8],
        &raw[8..12],
        &raw[12..16]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_known_vector() {
        // HMAC-SHA256("key", "The quick brown fox jumps over the lazy dog")
        let out = hmac_sha256(b"key", b"The quick brown fox jumps over the lazy dog");
        assert_eq!(
            hex::encode(out),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn pbkdf2_matches_rfc_vector() {
        // PBKDF2-HMAC-SHA256("password", "salt", 1) — ۳۲ بایت
        let d1 = derive("password", b"salt", 1);
        assert_eq!(
            hex::encode(d1),
            "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
        );
        let d2 = derive("password", b"salt", 2);
        assert_eq!(
            hex::encode(d2),
            "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43"
        );
    }

    #[test]
    fn hash_and_verify_roundtrip() {
        let hashed = hash_secret("رمز-تست").unwrap();
        assert!(verify_secret("رمز-تست", &hashed));
        assert!(!verify_secret("رمز-دیگر", &hashed));
        assert!(!verify_secret("", &hashed));
        assert!(!verify_secret("رمز-تست", "bad-format"));
        // هش دو بار متفاوت است چون salt تصادفی است
        assert_ne!(hashed, hash_secret("رمز-تست").unwrap());
    }

    #[test]
    fn recovery_code_format() {
        let code = generate_recovery_code();
        assert_eq!(code.len(), 19);
        assert_eq!(code.matches('-').count(), 3);
        assert!(code.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }
}
