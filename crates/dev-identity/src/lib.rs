use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const IDENTITY_SCHEMA_VERSION: u32 = 1;
pub const DEV_MARKER: &str = "deadbeef";
const NAMESPACE: &[u8] = b"fxdk-agent/dev-identity/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntheticDevIdentity {
    pub slot: u32,
    pub payload: String,
}

impl SyntheticDevIdentity {
    pub fn license(&self) -> String {
        format!("license:{}", self.payload)
    }

    pub fn license2(&self) -> String {
        format!("license2:{}", self.payload)
    }
}

#[derive(Debug, Clone)]
pub struct DevIdentityStore {
    path: PathBuf,
}

impl DevIdentityStore {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_local() -> Result<Self, DevIdentityError> {
        Ok(Self::at(default_identity_path()?))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn identity_for_slot(
        &self,
        slot: u32,
    ) -> Result<SyntheticDevIdentity, DevIdentityError> {
        if slot == 0 {
            return Err(DevIdentityError::InvalidSlot(slot));
        }

        let record = self.load_or_create()?;
        derive_identity(&record.seed, slot)
    }

    pub fn reset(&self) -> Result<(), DevIdentityError> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }

    fn load_or_create(&self) -> Result<IdentityRecord, DevIdentityError> {
        if self.path.exists() {
            return self.load();
        }

        let record = IdentityRecord::generate()?;
        self.save(&record)?;
        Ok(record)
    }

    fn load(&self) -> Result<IdentityRecord, DevIdentityError> {
        let bytes = fs::read(&self.path)?;
        let persisted: PersistedIdentity =
            serde_json::from_slice(&bytes)?;

        if persisted.schema_version != IDENTITY_SCHEMA_VERSION {
            return Err(DevIdentityError::UnsupportedSchema {
                found: persisted.schema_version,
                supported: IDENTITY_SCHEMA_VERSION,
            });
        }

        let seed = decode_seed(&persisted.seed)?;
        Ok(IdentityRecord { seed })
    }

    fn save(&self, record: &IdentityRecord) -> Result<(), DevIdentityError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let persisted = PersistedIdentity {
            schema_version: IDENTITY_SCHEMA_VERSION,
            seed: encode_hex(&record.seed),
        };
        let mut bytes = serde_json::to_vec_pretty(&persisted)?;
        bytes.push(b'\n');

        fs::write(&self.path, bytes)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct IdentityRecord {
    seed: [u8; 32],
}

impl IdentityRecord {
    fn generate() -> Result<Self, DevIdentityError> {
        let mut seed = [0_u8; 32];
        getrandom::fill(&mut seed).map_err(DevIdentityError::Random)?;
        Ok(Self { seed })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedIdentity {
    schema_version: u32,
    seed: String,
}

#[derive(Debug)]
pub enum DevIdentityError {
    LocalDataDirectoryUnavailable,
    InvalidSlot(u32),
    InvalidSeed,
    UnsupportedSchema { found: u32, supported: u32 },
    Random(getrandom::Error),
    Io(io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for DevIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalDataDirectoryUnavailable => {
                write!(formatter, "local data directory is unavailable")
            }
            Self::InvalidSlot(slot) => {
                write!(formatter, "client slot must be greater than zero, got {slot}")
            }
            Self::InvalidSeed => {
                write!(formatter, "persisted development identity seed is invalid")
            }
            Self::UnsupportedSchema { found, supported } => write!(
                formatter,
                "unsupported identity schema version {found}; supported version is {supported}"
            ),
            Self::Random(error) => write!(formatter, "OS random generation failed: {error}"),
            Self::Io(error) => write!(formatter, "identity store I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "identity store JSON is invalid: {error}"),
        }
    }
}

impl Error for DevIdentityError {}

impl From<io::Error> for DevIdentityError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for DevIdentityError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub fn default_identity_path() -> Result<PathBuf, DevIdentityError> {
    let base_dirs =
        BaseDirs::new().ok_or(DevIdentityError::LocalDataDirectoryUnavailable)?;

    Ok(base_dirs
        .data_local_dir()
        .join("FXDK Agent")
        .join("identity.json"))
}

pub fn derive_identity(
    seed: &[u8; 32],
    slot: u32,
) -> Result<SyntheticDevIdentity, DevIdentityError> {
    if slot == 0 {
        return Err(DevIdentityError::InvalidSlot(slot));
    }

    let mut hasher = Sha256::new();
    hasher.update(NAMESPACE);
    hasher.update(seed);
    hasher.update(slot.to_be_bytes());
    let digest = hasher.finalize();

    let payload = format!(
        "{DEV_MARKER}{slot:08x}{}",
        encode_hex(&digest[..12])
    );

    debug_assert_eq!(payload.len(), 40);
    debug_assert!(payload.bytes().all(|byte| byte.is_ascii_hexdigit()));

    Ok(SyntheticDevIdentity { slot, payload })
}

fn decode_seed(value: &str) -> Result<[u8; 32], DevIdentityError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(DevIdentityError::InvalidSeed);
    }

    let mut seed = [0_u8; 32];
    for (index, byte) in seed.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(&value[start..start + 2], 16)
            .map_err(|_| DevIdentityError::InvalidSeed)?;
    }

    Ok(seed)
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use fmt::Write;
        write!(&mut output, "{byte:02x}")
            .expect("writing to String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use std::{fs, process};

    use super::{
        DEV_MARKER, DevIdentityError, DevIdentityStore, derive_identity,
    };

    fn test_store(name: &str) -> DevIdentityStore {
        let root = std::env::temp_dir().join(format!(
            "fxdk-agent-dev-identity-test-{}-{name}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        DevIdentityStore::at(root.join("identity.json"))
    }

    #[test]
    fn identity_payload_is_exactly_40_lowercase_hex() {
        let seed = [0x2a; 32];
        let identity = derive_identity(&seed, 1).expect("derive identity");

        assert_eq!(identity.payload.len(), 40);
        assert!(identity.payload.starts_with(DEV_MARKER));
        assert!(identity
            .payload
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
        assert_eq!(&identity.payload[8..16], "00000001");
        assert_eq!(identity.license(), format!("license:{}", identity.payload));
        assert_eq!(identity.license2(), format!("license2:{}", identity.payload));
    }

    #[test]
    fn identity_is_deterministic_and_unique_per_slot() {
        let seed = [0x7b; 32];

        let first = derive_identity(&seed, 1).expect("slot 1");
        let again = derive_identity(&seed, 1).expect("slot 1 repeat");
        let second = derive_identity(&seed, 2).expect("slot 2");

        assert_eq!(first, again);
        assert_ne!(first.payload, second.payload);
        assert_eq!(&second.payload[8..16], "00000002");
    }

    #[test]
    fn zero_slot_is_rejected() {
        let error = derive_identity(&[0_u8; 32], 0)
            .expect_err("slot zero must fail");

        assert!(matches!(error, DevIdentityError::InvalidSlot(0)));
    }

    #[test]
    fn store_persists_identity_across_reloads() {
        let store = test_store("persistent");
        let first = store.identity_for_slot(1).expect("first identity");
        let second = DevIdentityStore::at(store.path())
            .identity_for_slot(1)
            .expect("reloaded identity");

        assert_eq!(first, second);
        assert!(store.path().is_file());
    }

    #[test]
    fn reset_rotates_identity_seed() {
        let store = test_store("reset");
        let first = store.identity_for_slot(1).expect("first identity");

        store.reset().expect("reset identity");
        let second = store.identity_for_slot(1).expect("second identity");

        assert_ne!(first.payload, second.payload);
    }
}
