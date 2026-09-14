//! The encrypted secrets vault: the one place API key *values* exist on disk,
//! and only as AEAD ciphertext under a key derived from the user's own vault
//! password. Everything else — settings, logs, command lines — keeps referring
//! to secrets by environment-variable *name*.
//!
//! Unlocking derives the key with PBKDF2-HMAC-SHA256 and authenticates it by
//! decrypting the AES-256-GCM payload: a wrong password is indistinguishable
//! from corruption and reported as such. While unlocked, the derived key (never
//! the password) is held in zeroized memory so entries can be re-encrypted
//! without asking again; dropping or relocking the vault scrubs both.
//!
//! Child processes receive values strictly by injection: when a runtime or MCP
//! server whitelists a variable name, [`resolve`] supplies the value from the
//! unlocked vault first and from the process environment as the fallback, so
//! the managed store always wins over a stale OS-level variable.

use std::{
    collections::BTreeMap,
    env, fs,
    io::Write,
    num::NonZeroU32,
    path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ring::{
    aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey},
    pbkdf2::{self, PBKDF2_HMAC_SHA256},
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::{RuntimeError, RuntimeSettings, tools};

/// File name of the vault next to `runtime.json` in the application config directory.
pub const VAULT_FILE_NAME: &str = "secrets.vault.json";

/// PBKDF2-HMAC-SHA256 work factor for new vaults (OWASP 2023 recommendation).
pub const DEFAULT_KDF_ITERATIONS: u32 = 600_000;

const VAULT_VERSION: u32 = 1;
const KDF_ALGORITHM: &str = "PBKDF2-HMAC-SHA256";
const CIPHER_ALGORITHM: &str = "AES-256-GCM";
const MIN_PASSWORD_LENGTH: usize = 8;

fn vault_error(message: impl Into<String>) -> RuntimeError {
    RuntimeError::Vault(message.into())
}

/// The decrypted key→value map held only while the vault is unlocked.
///
/// Values are individually zeroized; the map is cloned only to hand a snapshot
/// to one background task, which scrubs its copy when done.
#[derive(Clone, Default)]
pub struct SecretValues {
    entries: BTreeMap<String, Zeroizing<String>>,
}

impl std::fmt::Debug for SecretValues {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("SecretValues").field("names", &self.names().collect::<Vec<_>>()).finish()
    }
}

impl SecretValues {
    /// The value stored for one variable name, if any.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries.get(name).map(|value| value.as_str())
    }

    /// Sorted variable names held by the vault. Names are not secret: settings
    /// files already publish them.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Overlays every entry onto a child process command that otherwise
    /// inherits this process's environment (the supervised Codex App Server
    /// and the bridge). Same-source precedence does not apply here: the vault
    /// value is written over any inherited one, because an unlocked vault is
    /// the user's explicit, current source.
    pub fn overlay_on(&self, command: &mut std::process::Command) {
        for (name, value) in &self.entries {
            command.env(name, value.as_str());
        }
    }
}

/// Where a variable's value comes from when both the unlocked vault and the
/// process environment define it: the vault wins, the environment fills gaps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretSource {
    Vault,
    Environment,
}

/// Reports which source currently supplies one variable name, if any.
#[must_use]
pub fn secret_source(name: &str, vault: Option<&SecretValues>) -> Option<SecretSource> {
    if vault.and_then(|values| values.get(name)).is_some() {
        Some(SecretSource::Vault)
    } else {
        env::var(name).is_ok_and(|value| !value.is_empty()).then_some(SecretSource::Environment)
    }
}

/// Resolves one whitelisted variable name to its value: the unlocked vault
/// first, then the process environment. Returns `None` when neither has a
/// non-empty value.
pub(crate) fn resolve(name: &str, vault: Option<&SecretValues>) -> Option<Zeroizing<String>> {
    if let Some(value) = vault.and_then(|values| values.get(name)) {
        return Some(Zeroizing::new(value.to_owned()));
    }
    env::var(name).ok().filter(|value| !value.is_empty()).map(Zeroizing::new)
}

/// The user-facing guidance attached to every missing-variable error.
pub(crate) fn missing_variable_message(name: &str) -> String {
    format!("缺少环境变量 {name}：请设置该环境变量，或在「密钥保险库」中收录同名变量并解锁")
}

#[derive(Serialize, Deserialize)]
struct VaultFile {
    version: u32,
    kdf: KdfSection,
    cipher: CipherSection,
    /// Plaintext index of stored variable names, so the locked screen can show
    /// what the vault holds without decrypting. Never values.
    #[serde(default)]
    variables: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct KdfSection {
    algorithm: String,
    iterations: u32,
    salt: String,
}

#[derive(Serialize, Deserialize)]
struct CipherSection {
    algorithm: String,
    nonce: String,
    ciphertext: String,
}

/// An unlocked vault: the derived encryption key plus decrypted values, both
/// zeroized on drop. The password itself is discarded right after derivation.
pub struct UnlockedVault {
    path: PathBuf,
    key: Zeroizing<Vec<u8>>,
    salt: Vec<u8>,
    iterations: u32,
    values: SecretValues,
}

impl std::fmt::Debug for UnlockedVault {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UnlockedVault")
            .field("path", &self.path)
            .field("values", &self.values)
            .finish_non_exhaustive()
    }
}

impl UnlockedVault {
    /// The default vault location: `secrets.vault.json` next to `runtime.json`.
    #[must_use]
    pub fn default_path() -> PathBuf {
        RuntimeSettings::default_path().with_file_name(VAULT_FILE_NAME)
    }

    /// Whether a vault file exists at the path.
    #[must_use]
    pub fn exists(path: &Path) -> bool {
        path.is_file()
    }

    /// Reads only the plaintext variable-name index of a vault file, for the
    /// locked screen. Values stay sealed.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read or parsed.
    pub fn variable_names(path: &Path) -> Result<Vec<String>, RuntimeError> {
        Ok(read_vault_file(path)?.variables)
    }

    /// Creates a new vault protected by `password`. Fails rather than
    /// overwriting an existing vault file.
    ///
    /// # Errors
    ///
    /// Returns an error when the password is too weak, the file already
    /// exists, or writing fails.
    pub fn create(path: &Path, password: &str) -> Result<Self, RuntimeError> {
        if Self::exists(path) {
            return Err(vault_error("保险库文件已存在；如需重置请先备份并删除原文件"));
        }
        let derived = derive_new(password)?;
        let vault = Self {
            path: path.to_owned(),
            key: derived.key,
            salt: derived.salt,
            iterations: derived.iterations,
            values: SecretValues::default(),
        };
        vault.persist()?;
        Ok(vault)
    }

    /// Unlocks an existing vault. A wrong password fails AEAD authentication
    /// and is reported as "incorrect password or corrupted data".
    ///
    /// # Errors
    ///
    /// Returns an error when the file is missing, malformed, or the password
    /// does not authenticate.
    pub fn unlock(path: &Path, password: &str) -> Result<Self, RuntimeError> {
        let file = read_vault_file(path)?;
        if file.version != VAULT_VERSION
            || file.kdf.algorithm != KDF_ALGORITHM
            || file.cipher.algorithm != CIPHER_ALGORITHM
        {
            return Err(vault_error("保险库文件版本或算法不受支持"));
        }
        let salt = BASE64
            .decode(&file.kdf.salt)
            .map_err(|_| vault_error("保险库文件损坏（salt 编码无效）"))?;
        let iterations = file.kdf.iterations;
        let key = derive_key(password, &salt, iterations)?;
        let entries = decrypt_entries(&key, &file.cipher)?;
        Ok(Self { path: path.to_owned(), key, salt, iterations, values: entries })
    }

    /// The decrypted values, keyed by variable name.
    #[must_use]
    pub fn values(&self) -> &SecretValues {
        &self.values
    }

    /// Adds or replaces one entry and re-encrypts the vault immediately.
    /// The in-memory map is only updated after the write succeeds, so a failed
    /// write never leaves memory and disk disagreeing.
    ///
    /// # Errors
    ///
    /// Returns an error when the name is not a valid environment-variable
    /// name, the value is empty, or the encrypted write fails.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), RuntimeError> {
        if !tools::valid_env_name(name) {
            return Err(vault_error("变量名不合法：需以字母或下划线开头，仅含字母、数字、下划线"));
        }
        if value.is_empty() {
            return Err(vault_error("密钥值不能为空"));
        }
        let mut next = self.values.clone();
        next.entries.insert(name.to_owned(), Zeroizing::new(value.to_owned()));
        self.persist_values(&next)?;
        self.values = next;
        Ok(())
    }

    /// Removes one entry and re-encrypts; returns whether it existed.
    ///
    /// # Errors
    ///
    /// Returns an error when the encrypted write fails.
    pub fn remove(&mut self, name: &str) -> Result<bool, RuntimeError> {
        let mut next = self.values.clone();
        if next.entries.remove(name).is_none() {
            return Ok(false);
        }
        self.persist_values(&next)?;
        self.values = next;
        Ok(true)
    }

    /// Re-protects the vault with a new password: fresh salt, fresh key.
    ///
    /// # Errors
    ///
    /// Returns an error when the password is too weak or the write fails.
    pub fn change_password(&mut self, new_password: &str) -> Result<(), RuntimeError> {
        let derived = derive_new(new_password)?;
        self.persist_with(&derived.key, &derived.salt, derived.iterations, &self.values)?;
        self.key = derived.key;
        self.salt = derived.salt;
        self.iterations = derived.iterations;
        Ok(())
    }

    /// Encrypts and atomically writes the current entries.
    fn persist(&self) -> Result<(), RuntimeError> {
        self.persist_values(&self.values)
    }

    fn persist_values(&self, values: &SecretValues) -> Result<(), RuntimeError> {
        self.persist_with(&self.key, &self.salt, self.iterations, values)
    }

    fn persist_with(
        &self,
        key: &[u8],
        salt: &[u8],
        iterations: u32,
        values: &SecretValues,
    ) -> Result<(), RuntimeError> {
        let mut plaintext = serde_json::to_vec(
            &values.entries.iter().map(|(k, v)| (k, v.as_str())).collect::<BTreeMap<_, _>>(),
        )
        .map_err(|error| vault_error(format!("保险库序列化失败：{error}")))?;
        let (nonce, ciphertext) = encrypt(key, &plaintext)?;
        plaintext.zeroize();
        let file = VaultFile {
            version: VAULT_VERSION,
            kdf: KdfSection {
                algorithm: KDF_ALGORITHM.to_owned(),
                iterations,
                salt: BASE64.encode(salt),
            },
            cipher: CipherSection {
                algorithm: CIPHER_ALGORITHM.to_owned(),
                nonce: BASE64.encode(nonce),
                ciphertext: BASE64.encode(ciphertext),
            },
            variables: values.names().cloned().collect(),
        };
        write_vault_file(&self.path, &file)
    }
}

fn check_password(password: &str) -> Result<(), RuntimeError> {
    if password.chars().count() < MIN_PASSWORD_LENGTH {
        return Err(vault_error(format!("保险库密码至少需要 {MIN_PASSWORD_LENGTH} 个字符")));
    }
    Ok(())
}

/// A freshly derived password key together with its KDF parameters.
struct DerivedKey {
    key: Zeroizing<Vec<u8>>,
    salt: Vec<u8>,
    iterations: u32,
}

/// Derives a fresh key for a new password: new random salt each time.
fn derive_new(password: &str) -> Result<DerivedKey, RuntimeError> {
    check_password(password)?;
    let mut salt = vec![0_u8; 16];
    SystemRandom::new()
        .fill(&mut salt)
        .map_err(|_| vault_error("无法生成保险库随机盐"))?;
    let key = derive_key(password, &salt, DEFAULT_KDF_ITERATIONS)?;
    Ok(DerivedKey { key, salt, iterations: DEFAULT_KDF_ITERATIONS })
}

fn derive_key(
    password: &str,
    salt: &[u8],
    iterations: u32,
) -> Result<Zeroizing<Vec<u8>>, RuntimeError> {
    let iterations = NonZeroU32::new(iterations)
        .ok_or_else(|| vault_error("保险库 KDF 迭代数无效"))?;
    let password = Zeroizing::new(password.as_bytes().to_vec());
    let mut key = Zeroizing::new(vec![0_u8; 32]);
    pbkdf2::derive(PBKDF2_HMAC_SHA256, iterations, salt, &password, &mut key);
    Ok(key)
}

fn encrypt(key: &[u8], plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), RuntimeError> {
    let mut nonce_bytes = vec![0_u8; 12];
    SystemRandom::new()
        .fill(&mut nonce_bytes)
        .map_err(|_| vault_error("无法生成保险库随机数"))?;
    let cipher = LessSafeKey::new(
        UnboundKey::new(&AES_256_GCM, key).map_err(|_| vault_error("保险库密钥无效"))?,
    );
    let nonce = Nonce::try_assume_unique_for_key(&nonce_bytes)
        .map_err(|_| vault_error("保险库随机数无效"))?;
    let mut in_out = plaintext.to_vec();
    cipher
        .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| vault_error("保险库加密失败"))?;
    Ok((nonce_bytes, in_out))
}

fn decrypt_entries(
    key: &[u8],
    cipher: &CipherSection,
) -> Result<SecretValues, RuntimeError> {
    let wrong =
        || vault_error("保险库密码不正确，或保险库数据已损坏");
    let nonce_bytes = BASE64.decode(&cipher.nonce).map_err(|_| wrong())?;
    let mut in_out = BASE64.decode(&cipher.ciphertext).map_err(|_| wrong())?;
    let cipher_key = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).map_err(|_| wrong())?);
    let nonce = Nonce::try_assume_unique_for_key(&nonce_bytes).map_err(|_| wrong())?;
    let plaintext = cipher_key.open_in_place(nonce, Aad::empty(), &mut in_out).map_err(|_| wrong())?;
    let parsed: BTreeMap<String, String> =
        serde_json::from_slice(plaintext).map_err(|_| wrong())?;
    let mut entries = BTreeMap::new();
    for (name, value) in parsed {
        if !tools::valid_env_name(&name) {
            return Err(vault_error(format!("保险库包含非法变量名 `{name}`")));
        }
        entries.insert(name, Zeroizing::new(value));
    }
    Ok(SecretValues { entries })
}

fn read_vault_file(path: &Path) -> Result<VaultFile, RuntimeError> {
    let raw = fs::read_to_string(path)
        .map_err(|source| RuntimeError::ReadSettings { path: path.to_owned(), source })?;
    serde_json::from_str(&raw)
        .map_err(|source| RuntimeError::ParseSettings { path: path.to_owned(), source })
}

/// Atomic write: temporary file, flush+sync, rename; the temporary file is
/// removed when any step fails.
fn write_vault_file(path: &Path, file: &VaultFile) -> Result<(), RuntimeError> {
    let raw = serde_json::to_vec_pretty(file)
        .map_err(|error| vault_error(format!("保险库序列化失败：{error}")))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|source| RuntimeError::WriteSettings { path: parent.to_owned(), source })?;
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| {
        let mut handle = fs::File::create(&temporary)?;
        handle.write_all(&raw)?;
        handle.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|source| RuntimeError::WriteSettings { path: path.to_owned(), source })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault_path(label: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "circuitfabric-vault-{label}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn create_unlock_round_trip_and_no_plaintext_on_disk() {
        let path = vault_path("roundtrip");
        let mut vault = UnlockedVault::create(&path, "correct horse battery").expect("create");
        vault.set("OPENAI_API_KEY", "sk-test-value-123").expect("set");
        vault.set("MCP_TOKEN", "mcp-value-456").expect("set");
        drop(vault);

        let raw = fs::read_to_string(&path).expect("read vault file");
        assert!(!raw.contains("sk-test-value-123"), "values never appear in plaintext");
        assert!(!raw.contains("mcp-value-456"));
        assert!(raw.contains("OPENAI_API_KEY"), "the name index stays readable while locked");
        assert_eq!(
            UnlockedVault::variable_names(&path).expect("names"),
            ["MCP_TOKEN", "OPENAI_API_KEY"]
        );

        let reopened = UnlockedVault::unlock(&path, "correct horse battery").expect("unlock");
        assert_eq!(reopened.values().get("OPENAI_API_KEY"), Some("sk-test-value-123"));
        assert_eq!(reopened.values().get("MCP_TOKEN"), Some("mcp-value-456"));
        assert_eq!(reopened.values().len(), 2);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn wrong_password_and_tampering_fail_closed() {
        let path = vault_path("wrong-password");
        let mut vault = UnlockedVault::create(&path, "correct horse battery").expect("create");
        vault.set("OPENAI_API_KEY", "sk-test-value-123").expect("set");
        drop(vault);

        let error = UnlockedVault::unlock(&path, "wrong password").expect_err("must fail");
        assert!(error.to_string().contains("密码不正确"), "{error}");

        // Flip one byte inside the ciphertext field specifically: AEAD
        // authentication must reject it.
        let raw = fs::read_to_string(&path).expect("read");
        let mut parsed: serde_json::Value = serde_json::from_str(&raw).expect("parse");
        let ciphertext = parsed["cipher"]["ciphertext"].as_str().expect("ciphertext").to_owned();
        let flipped = format!("{}B", &ciphertext[..ciphertext.len() - 1]);
        parsed["cipher"]["ciphertext"] = serde_json::Value::String(flipped);
        fs::write(&path, serde_json::to_vec(&parsed).expect("encode")).expect("tamper");
        assert!(UnlockedVault::unlock(&path, "correct horse battery").is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn invalid_names_empty_values_and_weak_passwords_are_rejected() {
        let path = vault_path("validation");
        assert!(UnlockedVault::create(&path, "short").is_err());
        let mut vault = UnlockedVault::create(&path, "correct horse battery").expect("create");
        assert!(vault.set("3BAD", "value").is_err());
        assert!(vault.set("TOKEN=value", "value").is_err());
        assert!(vault.set("GOOD_NAME", "").is_err());
        assert!(vault.values().is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn change_password_rekeys_and_old_password_stops_working() {
        let path = vault_path("rekey");
        let mut vault = UnlockedVault::create(&path, "correct horse battery").expect("create");
        vault.set("OPENAI_API_KEY", "sk-test-value-123").expect("set");
        vault.change_password("new vault password").expect("rekey");
        drop(vault);

        assert!(UnlockedVault::unlock(&path, "correct horse battery").is_err());
        let reopened = UnlockedVault::unlock(&path, "new vault password").expect("unlock new");
        assert_eq!(reopened.values().get("OPENAI_API_KEY"), Some("sk-test-value-123"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_reencrypts_and_failed_remove_is_a_no_op() {
        let path = vault_path("remove");
        let mut vault = UnlockedVault::create(&path, "correct horse battery").expect("create");
        vault.set("OPENAI_API_KEY", "sk-test-value-123").expect("set");
        assert!(!vault.remove("NOT_THERE").expect("remove missing"));
        assert!(vault.remove("OPENAI_API_KEY").expect("remove"));
        drop(vault);
        let reopened = UnlockedVault::unlock(&path, "correct horse battery").expect("unlock");
        assert!(reopened.values().is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn resolve_prefers_the_unlocked_vault_then_the_environment() {
        let mut values = SecretValues::default();
        values.entries.insert("PATH".to_owned(), Zeroizing::new("vault-path".to_owned()));
        let resolved = resolve("PATH", Some(&values)).expect("vault wins over the environment");
        assert_eq!(resolved.as_str(), "vault-path");
        assert_eq!(secret_source("PATH", Some(&values)), Some(SecretSource::Vault));
        if env::var("PATH").is_ok_and(|value| !value.is_empty()) {
            assert_eq!(secret_source("PATH", None), Some(SecretSource::Environment));
        }
        let missing = "CIRCUITFABRIC_TEST_MISSING_9F3B71";
        assert!(resolve(missing, None).is_none());
        values.entries.insert(missing.to_owned(), Zeroizing::new("vault-value".to_owned()));
        let resolved = resolve(missing, Some(&values)).expect("vault fallback resolves");
        assert_eq!(resolved.as_str(), "vault-value");
        assert_eq!(secret_source(missing, Some(&values)), Some(SecretSource::Vault));
        assert_eq!(secret_source(missing, None), None);
    }

    #[test]
    fn creating_over_an_existing_vault_fails() {
        let path = vault_path("clobber");
        let _vault = UnlockedVault::create(&path, "correct horse battery").expect("create");
        assert!(UnlockedVault::create(&path, "another password").is_err());
        let _ = fs::remove_file(path);
    }
}
