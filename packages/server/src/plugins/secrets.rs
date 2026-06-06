use anyhow::Context;
use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use rand::Rng;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

const KEY_FILE: &str = "plugin-secrets.key";

pub fn encrypt(
    data_dir: &Path,
    user_id: &str,
    plugin_id: &str,
    value: &serde_json::Value,
) -> anyhow::Result<String> {
    let key = load_or_create_key(data_dir)?;
    let cipher = XChaCha20Poly1305::new((&key).into());
    let mut nonce = [0_u8; 24];
    rand::rng().fill_bytes(&mut nonce);
    let plaintext = serde_json::to_vec(value)?;
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: associated_data(user_id, plugin_id).as_bytes(),
            },
        )
        .map_err(|_| anyhow::anyhow!("failed to encrypt plugin secrets"))?;
    let mut encoded = nonce.to_vec();
    encoded.extend(ciphertext);
    Ok(STANDARD.encode(encoded))
}

pub fn decrypt(
    data_dir: &Path,
    user_id: &str,
    plugin_id: &str,
    encoded: &str,
) -> anyhow::Result<serde_json::Value> {
    let key = load_or_create_key(data_dir)?;
    let bytes = STANDARD
        .decode(encoded)
        .context("plugin secrets are not valid base64")?;
    if bytes.len() < 24 {
        anyhow::bail!("plugin secrets payload is truncated");
    }
    let (nonce, ciphertext) = bytes.split_at(24);
    let cipher = XChaCha20Poly1305::new((&key).into());
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: associated_data(user_id, plugin_id).as_bytes(),
            },
        )
        .map_err(|_| anyhow::anyhow!("failed to decrypt plugin secrets"))?;
    serde_json::from_slice(&plaintext).context("plugin secrets contain invalid JSON")
}

fn load_or_create_key(data_dir: &Path) -> anyhow::Result<[u8; 32]> {
    let path = data_dir.join(KEY_FILE);
    if path.exists() {
        let bytes =
            fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
        return bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("{} must contain exactly 32 bytes", path.display()));
    }

    fs::create_dir_all(data_dir)?;
    let mut key = [0_u8; 32];
    rand::rng().fill_bytes(&mut key);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(&key)?;
    file.sync_all()?;
    set_key_permissions(&path)?;
    Ok(key)
}

fn associated_data(user_id: &str, plugin_id: &str) -> String {
    format!("{user_id}\0{plugin_id}")
}

#[cfg(unix)]
fn set_key_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_key_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_round_trip_and_are_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let value = serde_json::json!({"token": "secret"});
        let encrypted = encrypt(directory.path(), "user-a", "plugin-a", &value).unwrap();
        assert!(!encrypted.contains("secret"));
        assert_eq!(
            decrypt(directory.path(), "user-a", "plugin-a", &encrypted).unwrap(),
            value
        );
        assert!(decrypt(directory.path(), "user-b", "plugin-a", &encrypted).is_err());
    }
}
