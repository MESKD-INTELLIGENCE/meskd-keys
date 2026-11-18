use std::fs;
use std::path::{Path, PathBuf};

use ssh_key::private::PrivateKey;
use ssh_key::public::{KeyData, PublicKey};
use ssh_key::{Algorithm, EcdsaCurve, HashAlg, LineEnding};

use crate::error::{Error, Result};

const SKIP: [&str; 4] = [
    "config",
    "known_hosts",
    "known_hosts.old",
    "authorized_keys",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyKind {
    Ed25519,
    Rsa4096,
}

impl KeyKind {
    pub const ALL: [KeyKind; 2] = [KeyKind::Ed25519, KeyKind::Rsa4096];

    pub fn label(self) -> &'static str {
        match self {
            KeyKind::Ed25519 => "Ed25519",
            KeyKind::Rsa4096 => "RSA 4096",
        }
    }

    fn algorithm(self) -> Algorithm {
        match self {
            KeyKind::Ed25519 => Algorithm::Ed25519,
            KeyKind::Rsa4096 => Algorithm::Rsa { hash: None },
        }
    }
}

#[derive(Clone, Debug)]
pub struct KeyInfo {
    pub name: String,
    pub kind: String,
    pub fingerprint: String,
    pub comment: String,
    pub encrypted: bool,
    pub private: Option<PathBuf>,
    pub public: Option<PathBuf>,
    pub public_text: Option<String>,
    pub note: Option<String>,
    pub problem: Option<String>,
}

impl KeyInfo {
    pub fn is_private(&self) -> bool {
        self.private.is_some()
    }

    pub fn permissions_too_open(&self) -> bool {
        self.note
            .as_deref()
            .is_some_and(|n| n.starts_with("permissions "))
    }
}

pub fn ssh_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh"))
}

pub fn scan(dir: &Path) -> Vec<KeyInfo> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut stems: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if SKIP.contains(&file_name.as_str()) {
            continue;
        }
        let stem = file_name
            .strip_suffix(".pub")
            .unwrap_or(&file_name)
            .to_owned();
        if stem.is_empty() || stem.starts_with('.') {
            continue;
        }
        if !stems.contains(&stem) {
            stems.push(stem);
        }
    }
    stems.sort();
    stems.iter().filter_map(|stem| inspect(dir, stem)).collect()
}

fn inspect(dir: &Path, stem: &str) -> Option<KeyInfo> {
    let private_path = dir.join(stem);
    let public_path = dir.join(format!("{stem}.pub"));
    let private = private_path
        .is_file()
        .then(|| fs::read(&private_path).ok())
        .flatten()
        .and_then(|bytes| PrivateKey::from_openssh(&bytes).ok());
    let public = public_path
        .is_file()
        .then(|| fs::read_to_string(&public_path).ok())
        .flatten()
        .and_then(|text| PublicKey::from_openssh(text.trim()).ok());

    if private.is_none() && public.is_none() {
        let text = fs::read_to_string(&private_path).ok()?;
        let looks_like_key = text.trim_start().starts_with("-----BEGIN")
            || ["ssh-", "ecdsa-", "sk-ssh-"]
                .iter()
                .any(|prefix| text.trim_start().starts_with(prefix));
        if !looks_like_key {
            return None;
        }
        return Some(KeyInfo {
            name: stem.to_owned(),
            kind: "unknown".to_owned(),
            fingerprint: String::new(),
            comment: String::new(),
            encrypted: false,
            private: private_path.is_file().then(|| private_path.clone()),
            public: None,
            public_text: None,
            note: None,
            problem: Some("not in OpenSSH format".to_owned()),
        });
    }

    let public_key = public
        .clone()
        .or_else(|| private.as_ref().map(|key| key.public_key().clone()))?;
    let encrypted = private.as_ref().is_some_and(|key| key.is_encrypted());
    let mode = private
        .is_some()
        .then(|| permissions(&private_path))
        .flatten();
    let note = if mode.is_some_and(|m| m & 0o077 != 0) {
        Some(format!(
            "permissions {:04o}, ssh may refuse this key",
            mode.unwrap_or(0)
        ))
    } else if private.is_some() && !encrypted {
        Some("no passphrase".to_owned())
    } else if private.is_none() && private_path.is_file() {
        Some("private key is not in OpenSSH format".to_owned())
    } else if private.is_none() {
        Some("public key only".to_owned())
    } else {
        None
    };

    Some(KeyInfo {
        name: stem.to_owned(),
        kind: describe(&public_key),
        fingerprint: public_key.fingerprint(HashAlg::Sha256).to_string(),
        comment: public_key.comment().to_owned(),
        encrypted,
        private: private.is_some().then(|| private_path.clone()),
        public: public.is_some().then(|| public_path.clone()),
        public_text: public_key.to_openssh().ok(),
        note,
        problem: None,
    })
}

fn describe(key: &PublicKey) -> String {
    match key.algorithm() {
        Algorithm::Ed25519 => "Ed25519".to_owned(),
        Algorithm::Ecdsa { curve } => match curve {
            EcdsaCurve::NistP256 => "ECDSA P-256".to_owned(),
            EcdsaCurve::NistP384 => "ECDSA P-384".to_owned(),
            EcdsaCurve::NistP521 => "ECDSA P-521".to_owned(),
        },
        Algorithm::Rsa { .. } => match key.key_data() {
            KeyData::Rsa(rsa) => rsa
                .n
                .as_positive_bytes()
                .map(|bytes| format!("RSA {} bit", bytes.len() * 8))
                .unwrap_or_else(|| "RSA".to_owned()),
            _ => "RSA".to_owned(),
        },
        Algorithm::Dsa => "DSA".to_owned(),
        Algorithm::SkEd25519 => "Ed25519-SK".to_owned(),
        Algorithm::SkEcdsaSha2NistP256 => "ECDSA-SK".to_owned(),
        other => other.as_str().to_owned(),
    }
}

pub fn generate(
    dir: &Path,
    name: &str,
    kind: KeyKind,
    passphrase: &str,
    comment: &str,
) -> Result<()> {
    validate_name(name)?;
    fs::create_dir_all(dir)?;
    let private_path = dir.join(name);
    let public_path = dir.join(format!("{name}.pub"));
    if private_path.exists() || public_path.exists() {
        return Err(Error::Exists);
    }
    let mut rng = rand_core::OsRng;
    let mut key = PrivateKey::random(&mut rng, kind.algorithm()).map_err(ssh_error)?;
    key.set_comment(if comment.trim().is_empty() {
        default_comment()
    } else {
        comment.trim().to_owned()
    });
    if !passphrase.is_empty() {
        key = key.encrypt(&mut rng, passphrase).map_err(ssh_error)?;
    }
    key.write_openssh_file(&private_path, LineEnding::LF)
        .map_err(ssh_error)?;
    key.public_key()
        .write_openssh_file(&public_path)
        .map_err(ssh_error)?;
    open_up_public(&public_path)?;
    Ok(())
}

pub fn import(dir: &Path, source: &Path) -> Result<String> {
    let bytes = fs::read(source)?;
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = sanitize(&stem);
    if name.is_empty() {
        return Err(Error::InvalidName);
    }
    fs::create_dir_all(dir)?;

    if let Ok(key) = PrivateKey::from_openssh(&bytes) {
        let private_path = dir.join(&name);
        let public_path = dir.join(format!("{name}.pub"));
        if private_path.exists() || public_path.exists() {
            return Err(Error::Exists);
        }
        key.write_openssh_file(&private_path, LineEnding::LF)
            .map_err(ssh_error)?;
        key.public_key()
            .write_openssh_file(&public_path)
            .map_err(ssh_error)?;
        open_up_public(&public_path)?;
        return Ok(format!("Imported {name}"));
    }

    let text = std::str::from_utf8(&bytes).map_err(|_| Error::NotAKey)?;
    let key = PublicKey::from_openssh(text.trim()).map_err(|_| Error::NotAKey)?;
    let public_path = dir.join(format!("{name}.pub"));
    if public_path.exists() {
        return Err(Error::Exists);
    }
    key.write_openssh_file(&public_path).map_err(ssh_error)?;
    open_up_public(&public_path)?;
    Ok(format!("Imported {name}"))
}

pub fn delete(info: &KeyInfo) -> Result<()> {
    let mut removed = false;
    for path in [&info.private, &info.public].into_iter().flatten() {
        match fs::remove_file(path) {
            Ok(()) => removed = true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    if removed {
        Ok(())
    } else {
        Err(Error::NotFound)
    }
}

pub fn tighten(info: &KeyInfo) -> Result<()> {
    let Some(path) = &info.private else {
        return Err(Error::NotFound);
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && !name.ends_with(".pub")
        && !name.ends_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn validate_name(name: &str) -> Result<()> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(Error::InvalidName)
    }
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .take(64)
        .collect::<String>()
        .trim_matches(['-', '.'])
        .to_owned()
}

fn default_comment() -> String {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    let host = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_default();
    match (user.is_empty(), host.is_empty()) {
        (false, false) => format!("{user}@{host}"),
        (false, true) => user,
        _ => String::new(),
    }
}

fn ssh_error(error: ssh_key::Error) -> Error {
    Error::Ssh(error.to_string())
}

#[cfg(unix)]
fn permissions(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .ok()
        .map(|meta| meta.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn permissions(_path: &Path) -> Option<u32> {
    None
}

fn open_up_public(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o644))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_and_lists_an_ed25519_key() {
        let dir = tempfile::tempdir().unwrap();
        generate(dir.path(), "id_test", KeyKind::Ed25519, "", "me@test").unwrap();
        let keys = scan(dir.path());
        assert_eq!(keys.len(), 1);
        let key = &keys[0];
        assert_eq!(key.name, "id_test");
        assert_eq!(key.kind, "Ed25519");
        assert_eq!(key.comment, "me@test");
        assert!(!key.encrypted);
        assert!(key.private.is_some() && key.public.is_some());
        assert!(key.fingerprint.starts_with("SHA256:"));
        assert!(
            key.public_text
                .as_deref()
                .unwrap()
                .starts_with("ssh-ed25519 ")
        );
        assert!(key.problem.is_none());
    }

    #[test]
    fn generated_private_key_is_encrypted_when_a_passphrase_is_given() {
        let dir = tempfile::tempdir().unwrap();
        generate(
            dir.path(),
            "id_locked",
            KeyKind::Ed25519,
            "correct horse",
            "",
        )
        .unwrap();
        let key = &scan(dir.path())[0];
        assert!(key.encrypted);
        let text = fs::read_to_string(key.private.as_ref().unwrap()).unwrap();
        assert!(text.contains("ENCRYPTED"));
    }

    #[test]
    fn refuses_bad_or_duplicate_names() {
        let dir = tempfile::tempdir().unwrap();
        for bad in ["", "../x", ".hidden", "has space", "id_test.pub"] {
            assert!(
                matches!(
                    generate(dir.path(), bad, KeyKind::Ed25519, "", ""),
                    Err(Error::InvalidName)
                ),
                "{bad} was accepted"
            );
        }
        generate(dir.path(), "id_test", KeyKind::Ed25519, "", "").unwrap();
        assert!(matches!(
            generate(dir.path(), "id_test", KeyKind::Ed25519, "", ""),
            Err(Error::Exists)
        ));
    }

    #[test]
    fn imports_private_and_public_keys() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        generate(source.path(), "id_source", KeyKind::Ed25519, "", "me").unwrap();
        import(target.path(), &source.path().join("id_source")).unwrap();
        import(target.path(), &source.path().join("id_source.pub")).unwrap_err();
        let keys = scan(target.path());
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].name, "id_source");
        assert_eq!(keys[0].comment, "me");

        let other = tempfile::tempdir().unwrap();
        let result = import(other.path(), &source.path().join("id_source.pub")).unwrap();
        assert!(result.contains("id_source"));
        assert!(scan(other.path())[0].private.is_none());
    }

    #[test]
    fn rejects_files_that_are_not_keys() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        fs::write(&file, "hello").unwrap();
        assert!(matches!(import(dir.path(), &file), Err(Error::NotAKey)));
    }

    #[test]
    fn deletes_both_halves_of_a_pair() {
        let dir = tempfile::tempdir().unwrap();
        generate(dir.path(), "id_gone", KeyKind::Ed25519, "", "").unwrap();
        let key = scan(dir.path()).remove(0);
        delete(&key).unwrap();
        assert!(scan(dir.path()).is_empty());
        assert!(matches!(delete(&key), Err(Error::NotFound)));
    }

    #[test]
    fn files_that_are_not_keys_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("config"), "Host x\n").unwrap();
        fs::write(dir.path().join("known_hosts"), "").unwrap();
        fs::write(dir.path().join("notes.txt"), "hello").unwrap();
        fs::write(dir.path().join(".DS_Store"), "x").unwrap();
        assert!(scan(dir.path()).is_empty());
    }

    #[test]
    fn unsupported_key_encodings_are_shown_with_a_reason() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("id_rsa_old"),
            "-----BEGIN RSA PRIVATE KEY-----\nAAAA\n-----END RSA PRIVATE KEY-----\n",
        )
        .unwrap();
        let key = scan(dir.path()).remove(0);
        assert_eq!(key.problem.as_deref(), Some("not in OpenSSH format"));
        assert!(key.public_text.is_none());
    }

    #[test]
    fn names_are_sanitised_on_import() {
        assert_eq!(sanitize("my key!"), "my-key");
        assert_eq!(sanitize("..x.."), "x");
        assert_eq!(sanitize("id_ed25519"), "id_ed25519");
    }

    #[cfg(unix)]
    #[test]
    fn loose_permissions_are_reported_and_fixed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        generate(dir.path(), "id_loose", KeyKind::Ed25519, "", "").unwrap();
        let private = dir.path().join("id_loose");
        fs::set_permissions(&private, fs::Permissions::from_mode(0o644)).unwrap();
        let key = scan(dir.path()).remove(0);
        assert!(key.permissions_too_open());
        tighten(&key).unwrap();
        let key = &scan(dir.path())[0];
        assert!(!key.permissions_too_open());
    }
}
