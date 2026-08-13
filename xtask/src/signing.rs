use std::fs;
use std::io::Cursor;
use std::path::Path;

use minisign::{PublicKey, SecretKey, SecretKeyBox, SignatureBox};

use crate::{Result, XtaskError};

pub fn sign_file(input: &Path, secret_key_path: &Path, password: Option<String>) -> Result<String> {
    let private_key = fs::read_to_string(secret_key_path).map_err(|error| {
        XtaskError::io(
            "signing_key_read_failed",
            &secret_key_path.display().to_string(),
            error,
        )
    })?;
    sign_file_with_private_key(input, &private_key, password)
}

pub fn sign_file_with_private_key(
    input: &Path,
    private_key: &str,
    password: Option<String>,
) -> Result<String> {
    let boxed = SecretKeyBox::from_string(private_key)
        .map_err(|error| XtaskError::failure("signing_key_invalid", error.to_string()))?;
    let secret = match password {
        Some(password) => SecretKey::from_box(boxed, Some(password)),
        None => SecretKey::from_unencrypted_box(boxed),
    }
    .map_err(|error| XtaskError::failure("signing_key_invalid", error.to_string()))?;
    let public = PublicKey::from_secret_key(&secret)
        .map_err(|error| XtaskError::failure("signing_key_invalid", error.to_string()))?;
    let bytes = fs::read(input).map_err(|error| {
        XtaskError::io(
            "release_package_read_failed",
            &input.display().to_string(),
            error,
        )
    })?;
    let signature = minisign::sign(
        Some(&public),
        &secret,
        Cursor::new(&bytes),
        None,
        Some("LiliaCode updater signature"),
    )
    .map_err(|error| XtaskError::failure("release_sign_failed", error.to_string()))?;
    minisign::verify(&public, &signature, Cursor::new(&bytes), true, false, false).map_err(
        |error| XtaskError::failure("release_signature_roundtrip_failed", error.to_string()),
    )?;
    Ok(signature.to_string())
}

pub fn verify_bytes(public: &PublicKey, signature: &str, bytes: &[u8]) -> Result {
    let signature = SignatureBox::from_string(signature)
        .map_err(|error| XtaskError::failure("signature_invalid", error.to_string()))?;
    minisign::verify(public, &signature, Cursor::new(bytes), true, false, false)
        .map_err(|error| XtaskError::failure("signature_verification_failed", error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use minisign::KeyPair;

    #[test]
    fn signs_and_verifies_updater_payload() {
        let keys = KeyPair::generate_unencrypted_keypair().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let payload = directory.path().join("update.zip");
        let secret = directory.path().join("minisign.key");
        fs::write(&payload, b"release payload").unwrap();
        fs::write(&secret, keys.sk.to_box(None).unwrap().to_string()).unwrap();
        let signature = sign_file(&payload, &secret, None).unwrap();
        verify_bytes(&keys.pk, &signature, b"release payload").unwrap();
        assert!(verify_bytes(&keys.pk, &signature, b"tampered").is_err());
        let boxed = SecretKeyBox::from_string(&fs::read_to_string(secret).unwrap()).unwrap();
        let loaded = SecretKey::from_unencrypted_box(boxed).unwrap();
        assert_eq!(PublicKey::from_secret_key(&loaded).unwrap(), keys.pk);
    }
}
