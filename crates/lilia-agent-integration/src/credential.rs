//! Product Credential Broker bridge (#50).
//!
//! Official login / API key material enters only through AgentKit Credential Broker.
//! Provider instances and profiles store CredentialRef only — never secret material.
//! Credential health is diagnosed independently from Native Runtime health.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mutsuki_agent_contracts::{
    official_credential_providers, CredentialCapability, CredentialDescriptor,
    CredentialImportRequest, CredentialKind, CredentialLoginRequest, CredentialMaterialOrigin,
    CredentialProviderDescriptor, CredentialRef, CredentialRefreshPolicy, CredentialRevocationInfo,
    CredentialRevokeRequest, CredentialStatus, CredentialStatusRequest,
    ANTHROPIC_CREDENTIAL_PROVIDER_ID, CREDENTIAL_UNSUPPORTED_FOR_CUSTOM_RUNTIME,
    OPENAI_CREDENTIAL_PROVIDER_ID,
};
use mutsuki_agent_runtime::{CredentialBrokerService, InMemorySecretStore, SecretStore};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::NativeRuntimeError;

/// Product-facing credential login request (secret accepted only on this boundary).
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductCredentialLoginInput {
    pub provider_id: String,
    pub kind: CredentialKind,
    pub secret_material: String,
    #[serde(default)]
    pub account_label: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

impl fmt::Debug for ProductCredentialLoginInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductCredentialLoginInput")
            .field("provider_id", &self.provider_id)
            .field("kind", &self.kind)
            .field("secret_material", &"[REDACTED]")
            .field("account_label", &self.account_label)
            .field("source", &self.source)
            .finish()
    }
}

/// Product-facing import for official-login-generated API keys.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductCredentialImportInput {
    pub provider_id: String,
    pub kind: CredentialKind,
    pub secret_material: String,
    #[serde(default)]
    pub account_label: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub permissions_summary: Option<String>,
    #[serde(default)]
    pub independent_revoke_uri: Option<String>,
}

impl fmt::Debug for ProductCredentialImportInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductCredentialImportInput")
            .field("provider_id", &self.provider_id)
            .field("kind", &self.kind)
            .field("secret_material", &"[REDACTED]")
            .field("account_label", &self.account_label)
            .field("source", &self.source)
            .field("permissions_summary", &self.permissions_summary)
            .field("independent_revoke_uri", &self.independent_revoke_uri)
            .finish()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialHealthSnapshot {
    pub broker_ready: bool,
    pub broker_degraded: bool,
    pub recovery_issues: Vec<ProductCredentialRecoveryIssue>,
    pub provider_count: usize,
    pub credential_count: usize,
    pub active_count: usize,
    pub unavailable_count: usize,
    /// True when at least one usable API credential can bind a Provider instance.
    pub has_usable_model_credential: bool,
    pub credentials: Vec<CredentialDescriptorView>,
}

/// Public credential view — never includes secret material.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialDescriptorView {
    pub credential_id: String,
    pub revision: u64,
    pub provider_id: String,
    pub kind: CredentialKind,
    pub status: CredentialStatus,
    pub account_label: Option<String>,
    pub source: Option<String>,
    pub model_inference: bool,
}

impl From<&CredentialDescriptor> for CredentialDescriptorView {
    fn from(value: &CredentialDescriptor) -> Self {
        Self {
            credential_id: value.credential.credential_id.clone(),
            revision: value.credential.revision,
            provider_id: value.provider_id.clone(),
            kind: value.kind,
            status: value.status,
            account_label: value.account_label.clone(),
            source: value.source.clone(),
            model_inference: value.capability.model_inference,
        }
    }
}

/// Separates credential diagnosis from Runtime capability diagnosis (#50 / #121 DoD).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndependentDiagnostics {
    pub credential: CredentialHealthSnapshot,
    pub runtime_backend: String,
    pub runtime_ready: bool,
    pub official_agent_server: bool,
    pub node_runner_default: bool,
    pub profile_id: Option<String>,
    pub profile_has_credential_refs: bool,
    pub credential_and_runtime_independent: bool,
    /// Honest: product turns use protocol HTTP Model Adapter when openai-compatible
    /// or anthropic-messages CredentialRef is bound (otherwise reference coding path).
    pub live_model_adapter_drives_turn: bool,
}

/// Secret-free persistence row used to rebuild the Credential Broker after restart.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductCredentialRecord {
    pub descriptor: CredentialDescriptor,
    pub secret_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation_intent: Option<ProductCredentialRevocationIntent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductCredentialRevocationIntent {
    pub requested_revision: u64,
    pub requested_at_unix_ms: u64,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductCredentialRecoveryIssue {
    pub credential_id: Option<String>,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProductCredentialRegistryLoad {
    pub records: Vec<ProductCredentialRecord>,
    pub issues: Vec<ProductCredentialRecoveryIssue>,
}

/// Host-neutral descriptor registry. Implementations must commit each upsert atomically.
pub trait ProductCredentialRegistry: Send + Sync {
    fn load(&self) -> Result<ProductCredentialRegistryLoad, NativeRuntimeError>;
    fn upsert(&self, record: &ProductCredentialRecord) -> Result<(), NativeRuntimeError>;
}

#[derive(Clone, Default)]
pub struct InMemoryProductCredentialRegistry {
    records: Arc<Mutex<BTreeMap<String, ProductCredentialRecord>>>,
}

impl ProductCredentialRegistry for InMemoryProductCredentialRegistry {
    fn load(&self) -> Result<ProductCredentialRegistryLoad, NativeRuntimeError> {
        Ok(ProductCredentialRegistryLoad {
            records: self
                .records
                .lock()
                .map_err(|_| NativeRuntimeError::Agent("credential registry lock poisoned".into()))?
                .values()
                .cloned()
                .collect(),
            issues: Vec::new(),
        })
    }

    fn upsert(&self, record: &ProductCredentialRecord) -> Result<(), NativeRuntimeError> {
        self.records
            .lock()
            .map_err(|_| NativeRuntimeError::Agent("credential registry lock poisoned".into()))?
            .insert(
                record.descriptor.credential.credential_id.clone(),
                record.clone(),
            );
        Ok(())
    }
}

#[derive(Clone)]
pub struct SqliteProductCredentialRegistry {
    connection: Arc<Mutex<Connection>>,
}

impl SqliteProductCredentialRegistry {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, NativeRuntimeError> {
        let connection = Connection::open(path).map_err(registry_storage_error)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> Result<Self, NativeRuntimeError> {
        let connection = Connection::open_in_memory().map_err(registry_storage_error)?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self, NativeRuntimeError> {
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(registry_storage_error)?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
            )
            .map_err(registry_storage_error)?;
        connection
            .execute_batch(
                r#"CREATE TABLE IF NOT EXISTS product_credential_registry (
                       credential_id TEXT PRIMARY KEY,
                       secret_id TEXT NOT NULL,
                       descriptor_json TEXT NOT NULL,
                       revocation_intent_json TEXT
                   );"#,
            )
            .map_err(registry_storage_error)?;
        if !sqlite_table_has_column(
            &connection,
            "product_credential_registry",
            "revocation_intent_json",
        )? {
            connection
                .execute(
                    "ALTER TABLE product_credential_registry ADD COLUMN revocation_intent_json TEXT",
                    [],
                )
                .map_err(registry_storage_error)?;
        }
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }
}

impl ProductCredentialRegistry for SqliteProductCredentialRegistry {
    fn load(&self) -> Result<ProductCredentialRegistryLoad, NativeRuntimeError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| NativeRuntimeError::Agent("credential registry lock poisoned".into()))?;
        let mut statement = connection
            .prepare(
                "SELECT credential_id, secret_id, descriptor_json, revocation_intent_json \
                 FROM product_credential_registry ORDER BY credential_id ASC",
            )
            .map_err(registry_storage_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(registry_storage_error)?;
        let mut loaded = ProductCredentialRegistryLoad::default();
        for row in rows {
            let (credential_id, secret_id, descriptor_json, revocation_intent_json) = match row {
                Ok(row) => row,
                Err(error) => {
                    loaded.issues.push(ProductCredentialRecoveryIssue {
                        credential_id: None,
                        code: "credential_registry_row_invalid".into(),
                        message: format!("a credential registry row could not be read: {error}"),
                    });
                    continue;
                }
            };
            let descriptor: CredentialDescriptor = match serde_json::from_str(&descriptor_json) {
                Ok(descriptor) => descriptor,
                Err(error) => {
                    loaded.issues.push(ProductCredentialRecoveryIssue {
                        credential_id: Some(credential_id),
                        code: "credential_descriptor_invalid".into(),
                        message: format!("credential metadata could not be decoded: {error}"),
                    });
                    continue;
                }
            };
            if descriptor.credential.credential_id != credential_id {
                loaded.issues.push(ProductCredentialRecoveryIssue {
                    credential_id: Some(credential_id),
                    code: "credential_id_mismatch".into(),
                    message: "credential metadata id does not match its registry key".into(),
                });
                continue;
            }
            let revocation_intent = match revocation_intent_json {
                Some(value) => match serde_json::from_str(&value) {
                    Ok(intent) => Some(intent),
                    Err(error) => {
                        loaded.issues.push(ProductCredentialRecoveryIssue {
                            credential_id: Some(descriptor.credential.credential_id.clone()),
                            code: "credential_revocation_intent_invalid".into(),
                            message: format!(
                                "credential revocation recovery metadata is invalid: {error}"
                            ),
                        });
                        continue;
                    }
                },
                None => None,
            };
            loaded.records.push(ProductCredentialRecord {
                descriptor,
                secret_id,
                revocation_intent,
            });
        }
        Ok(loaded)
    }

    fn upsert(&self, record: &ProductCredentialRecord) -> Result<(), NativeRuntimeError> {
        let descriptor_json = serde_json::to_string(&record.descriptor).map_err(|error| {
            NativeRuntimeError::Agent(format!(
                "encode credential descriptor `{}`: {error}",
                record.descriptor.credential.credential_id
            ))
        })?;
        let revocation_intent_json = record
            .revocation_intent
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| {
                NativeRuntimeError::Agent(format!(
                    "encode credential revocation intent `{}`: {error}",
                    record.descriptor.credential.credential_id
                ))
            })?;
        self.connection
            .lock()
            .map_err(|_| NativeRuntimeError::Agent("credential registry lock poisoned".into()))?
            .execute(
                r#"INSERT INTO product_credential_registry
                       (credential_id, secret_id, descriptor_json, revocation_intent_json)
                   VALUES (?1, ?2, ?3, ?4)
                   ON CONFLICT(credential_id) DO UPDATE SET
                       secret_id = excluded.secret_id,
                       descriptor_json = excluded.descriptor_json,
                       revocation_intent_json = excluded.revocation_intent_json"#,
                params![
                    &record.descriptor.credential.credential_id,
                    &record.secret_id,
                    descriptor_json,
                    revocation_intent_json
                ],
            )
            .map_err(registry_storage_error)?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct ProductCredentialBridge {
    broker: CredentialBrokerService,
    registry: Arc<dyn ProductCredentialRegistry>,
    /// Track known descriptors so product can list/diagnose without a Broker list API.
    known: Arc<Mutex<BTreeMap<String, CredentialDescriptor>>>,
    recovery_issues: Arc<Mutex<Vec<ProductCredentialRecoveryIssue>>>,
    lifecycle: Arc<Mutex<()>>,
}

impl Default for ProductCredentialBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl ProductCredentialBridge {
    pub fn new() -> Self {
        Self::with_persistence(
            Arc::new(InMemorySecretStore::default()),
            Arc::new(InMemoryProductCredentialRegistry::default()),
        )
        .expect("empty in-memory credential persistence")
    }

    pub fn with_secret_store(secrets: Arc<dyn SecretStore>) -> Result<Self, NativeRuntimeError> {
        Self::with_persistence(
            secrets,
            Arc::new(InMemoryProductCredentialRegistry::default()),
        )
    }

    pub fn with_persistence(
        secrets: Arc<dyn SecretStore>,
        registry: Arc<dyn ProductCredentialRegistry>,
    ) -> Result<Self, NativeRuntimeError> {
        let broker = CredentialBrokerService::new(secrets.clone());
        let loaded = registry.load()?;
        let mut known = BTreeMap::new();
        let mut recovery_issues = loaded.issues;
        for record in loaded.records {
            let original_record = record.clone();
            let credential_id = record.descriptor.credential.credential_id.clone();
            let secret_state = secrets.get(&record.secret_id);
            let secret_present = match secret_state {
                Ok(secret) => secret.is_some(),
                Err(error) => {
                    recovery_issues.push(ProductCredentialRecoveryIssue {
                        credential_id: Some(credential_id),
                        code: "credential_secret_store_unavailable".into(),
                        message: format!("credential secure storage is unavailable: {error}"),
                    });
                    continue;
                }
            };

            let mut descriptor = record.descriptor;
            let mut keep_intent = record.revocation_intent.clone();
            if descriptor.status != CredentialStatus::Revoked && !secret_present {
                descriptor = recovered_revoked_descriptor(
                    descriptor,
                    record.revocation_intent.as_ref(),
                    "local credential material is unavailable",
                )?;
                recovery_issues.push(ProductCredentialRecoveryIssue {
                    credential_id: Some(credential_id.clone()),
                    code: "credential_secret_missing".into(),
                    message: "credential material is missing and the credential was disabled"
                        .into(),
                });
                keep_intent = None;
            }

            if let Err(error) = broker.restore_descriptor(descriptor.clone(), &record.secret_id) {
                recovery_issues.push(ProductCredentialRecoveryIssue {
                    credential_id: Some(credential_id),
                    code: "credential_restore_rejected".into(),
                    message: format!("credential metadata was rejected during restore: {error}"),
                });
                continue;
            }

            if let Some(intent) = record.revocation_intent {
                if descriptor.status != CredentialStatus::Revoked {
                    match broker.revoke(CredentialRevokeRequest {
                        credential: descriptor.credential.clone(),
                        reason: intent.reason.clone(),
                    }) {
                        Ok(result) => {
                            descriptor = result.descriptor;
                            keep_intent = None;
                        }
                        Err(error) => {
                            if let Ok(current) = broker.descriptor_by_id(&credential_id) {
                                descriptor = current;
                            }
                            recovery_issues.push(ProductCredentialRecoveryIssue {
                                credential_id: Some(credential_id.clone()),
                                code: "credential_revocation_replay_failed".into(),
                                message: format!(
                                    "credential revocation will be retried after secure storage recovers: {error}"
                                ),
                            });
                        }
                    }
                } else if secret_present {
                    match secrets.delete(&record.secret_id) {
                        Ok(()) => keep_intent = None,
                        Err(error) => recovery_issues.push(ProductCredentialRecoveryIssue {
                            credential_id: Some(credential_id.clone()),
                            code: "credential_secret_cleanup_failed".into(),
                            message: format!(
                                "revoked credential material could not be removed: {error}"
                            ),
                        }),
                    }
                } else {
                    keep_intent = None;
                }
            }

            let recovered_record = ProductCredentialRecord {
                descriptor: descriptor.clone(),
                secret_id: record.secret_id,
                revocation_intent: keep_intent,
            };
            if recovered_record != original_record {
                if let Err(error) = registry.upsert(&recovered_record) {
                    recovery_issues.push(ProductCredentialRecoveryIssue {
                        credential_id: Some(credential_id.clone()),
                        code: "credential_recovery_persist_failed".into(),
                        message: format!("credential recovery state could not be saved: {error}"),
                    });
                }
            }
            known.insert(credential_id, descriptor);
        }
        Ok(Self {
            broker,
            registry,
            known: Arc::new(Mutex::new(known)),
            recovery_issues: Arc::new(Mutex::new(recovery_issues)),
            lifecycle: Arc::new(Mutex::new(())),
        })
    }

    pub fn broker(&self) -> &CredentialBrokerService {
        &self.broker
    }

    pub fn providers(&self) -> Vec<CredentialProviderDescriptor> {
        let mut providers = self.broker.providers();
        if providers.is_empty() {
            providers = official_credential_providers();
        }
        providers
    }

    pub fn login(
        &self,
        input: ProductCredentialLoginInput,
    ) -> Result<CredentialDescriptorView, NativeRuntimeError> {
        let _lifecycle = self.lifecycle_guard()?;
        let result = self
            .broker
            .login(CredentialLoginRequest {
                provider_id: input.provider_id,
                kind: input.kind,
                secret_material: input.secret_material,
                account_label: input.account_label,
                source: input.source,
                capability: CredentialCapability::default(),
                refresh_policy: CredentialRefreshPolicy::default(),
                expires_at_unix_ms: None,
                metadata: Value::Null,
            })
            .map_err(map_broker_error)?;
        self.commit_new_descriptor(result.descriptor)
    }

    pub fn import_generated_api_key(
        &self,
        input: ProductCredentialImportInput,
    ) -> Result<CredentialDescriptorView, NativeRuntimeError> {
        let _lifecycle = self.lifecycle_guard()?;
        let result = self
            .broker
            .import(CredentialImportRequest {
                provider_id: input.provider_id,
                kind: input.kind,
                secret_material: input.secret_material,
                origin: CredentialMaterialOrigin::OfficialLoginGenerated,
                account_label: input.account_label,
                source: input.source,
                permissions_summary: input.permissions_summary,
                independent_revoke_uri: input.independent_revoke_uri,
                capability: CredentialCapability::default(),
                refresh_policy: CredentialRefreshPolicy::default(),
                expires_at_unix_ms: None,
                metadata: json!({}),
            })
            .map_err(map_broker_error)?;
        self.commit_new_descriptor(result.descriptor)
    }

    pub fn status(
        &self,
        credential: CredentialRef,
    ) -> Result<CredentialDescriptorView, NativeRuntimeError> {
        let _lifecycle = self.lifecycle_guard()?;
        let result = self
            .broker
            .status(CredentialStatusRequest { credential })
            .map_err(map_broker_error)?;
        self.remember(&result.descriptor);
        self.persist(&result.descriptor)?;
        Ok(CredentialDescriptorView::from(&result.descriptor))
    }

    pub fn revoke(
        &self,
        credential: CredentialRef,
        reason: Option<String>,
    ) -> Result<CredentialDescriptorView, NativeRuntimeError> {
        let _lifecycle = self.lifecycle_guard()?;
        let descriptor = self
            .known
            .lock()
            .map_err(|_| NativeRuntimeError::Agent("credential known lock poisoned".into()))?
            .get(&credential.credential_id)
            .cloned()
            .ok_or_else(|| NativeRuntimeError::Agent("credential is unavailable".into()))?;
        if descriptor.credential.revision != credential.revision {
            return Err(NativeRuntimeError::Agent(
                "credential revision mismatch during revoke".into(),
            ));
        }
        let secret_id = self
            .broker
            .descriptor_secret_id(&credential)
            .map_err(map_broker_error)?;
        let intent = ProductCredentialRevocationIntent {
            requested_revision: credential.revision,
            requested_at_unix_ms: unix_now_ms(),
            reason: reason.clone(),
        };
        self.registry.upsert(&ProductCredentialRecord {
            descriptor,
            secret_id: secret_id.clone(),
            revocation_intent: Some(intent.clone()),
        })?;
        let credential_id = credential.credential_id.clone();
        match self
            .broker
            .revoke(CredentialRevokeRequest { credential, reason })
        {
            Ok(result) => {
                self.remember(&result.descriptor);
                self.persist(&result.descriptor)?;
                Ok(CredentialDescriptorView::from(&result.descriptor))
            }
            Err(error) => {
                let broker_error = map_broker_error(error);
                let mut recovery_message = format!(
                    "credential revocation is pending secure storage cleanup: {broker_error}"
                );
                if let Ok(current) = self.broker.descriptor_by_id(&credential_id) {
                    self.remember(&current);
                    if let Err(persist_error) = self.registry.upsert(&ProductCredentialRecord {
                        descriptor: current,
                        secret_id,
                        revocation_intent: Some(intent),
                    }) {
                        recovery_message.push_str(&format!(
                            "; updated revocation state could not be saved: {persist_error}"
                        ));
                    }
                }
                self.recovery_issues
                    .lock()
                    .map_err(|_| {
                        NativeRuntimeError::Agent("credential recovery issue lock poisoned".into())
                    })?
                    .push(ProductCredentialRecoveryIssue {
                        credential_id: Some(credential_id),
                        code: "credential_revocation_incomplete".into(),
                        message: recovery_message,
                    });
                Err(broker_error)
            }
        }
    }

    /// Prove Adapter path can resolve without leaking secret into descriptors.
    pub fn resolve_for_adapter(
        &self,
        credential: &CredentialRef,
    ) -> Result<(), NativeRuntimeError> {
        let _secret = self
            .broker
            .resolve_secret(credential)
            .map_err(map_broker_error)?;
        Ok(())
    }

    pub fn health(&self) -> CredentialHealthSnapshot {
        let known = self.known.lock().expect("credential known lock");
        let recovery_issues = self
            .recovery_issues
            .lock()
            .expect("credential recovery issue lock")
            .clone();
        let credentials: Vec<_> = known.values().map(CredentialDescriptorView::from).collect();
        let active_count = credentials
            .iter()
            .filter(|c| c.status == CredentialStatus::Active)
            .count();
        let unavailable_count = credentials.len().saturating_sub(active_count);
        let has_usable_model_credential = credentials.iter().any(|c| {
            c.status == CredentialStatus::Active
                && c.model_inference
                && (c.provider_id == OPENAI_CREDENTIAL_PROVIDER_ID
                    || c.provider_id == ANTHROPIC_CREDENTIAL_PROVIDER_ID)
        });
        CredentialHealthSnapshot {
            broker_ready: true,
            broker_degraded: !recovery_issues.is_empty(),
            recovery_issues,
            provider_count: self.providers().len(),
            credential_count: credentials.len(),
            active_count,
            unavailable_count,
            has_usable_model_credential,
            credentials,
        }
    }

    pub fn primary_usable_credential(&self) -> Option<CredentialRef> {
        let known = self.known.lock().expect("credential known lock");
        known.values().find_map(|descriptor| {
            if descriptor.status == CredentialStatus::Active
                && descriptor.capability.model_inference
            {
                Some(descriptor.credential.clone())
            } else {
                None
            }
        })
    }

    pub fn openai_compatible_bindings(&self) -> Vec<(String, CredentialRef)> {
        let known = self.known.lock().expect("credential known lock");
        known
            .values()
            .filter(|descriptor| {
                descriptor.status == CredentialStatus::Active
                    && descriptor.provider_id == OPENAI_CREDENTIAL_PROVIDER_ID
            })
            .enumerate()
            .map(|(index, descriptor)| {
                (
                    format!("openai-compatible-{}", index + 1),
                    descriptor.credential.clone(),
                )
            })
            .collect()
    }

    pub fn primary_anthropic_credential(&self) -> Option<CredentialRef> {
        let known = self.known.lock().expect("credential known lock");
        known.values().find_map(|descriptor| {
            if descriptor.status == CredentialStatus::Active
                && descriptor.capability.model_inference
                && descriptor.provider_id == ANTHROPIC_CREDENTIAL_PROVIDER_ID
            {
                Some(descriptor.credential.clone())
            } else {
                None
            }
        })
    }

    fn remember(&self, descriptor: &CredentialDescriptor) {
        self.known.lock().expect("credential known lock").insert(
            descriptor.credential.credential_id.clone(),
            descriptor.clone(),
        );
    }

    fn commit_new_descriptor(
        &self,
        descriptor: CredentialDescriptor,
    ) -> Result<CredentialDescriptorView, NativeRuntimeError> {
        if let Err(error) = self.persist(&descriptor) {
            let rollback = self
                .broker
                .discard_for_rollback(&descriptor.credential)
                .map_err(map_broker_error);
            return match rollback {
                Ok(()) => Err(error),
                Err(rollback_error) => Err(NativeRuntimeError::Agent(format!(
                    "{error}; credential rollback failed: {rollback_error}"
                ))),
            };
        }
        self.remember(&descriptor);
        Ok(CredentialDescriptorView::from(&descriptor))
    }

    fn persist(&self, descriptor: &CredentialDescriptor) -> Result<(), NativeRuntimeError> {
        let secret_id = self
            .broker
            .descriptor_secret_id(&descriptor.credential)
            .map_err(map_broker_error)?;
        self.registry.upsert(&ProductCredentialRecord {
            descriptor: descriptor.clone(),
            secret_id,
            revocation_intent: None,
        })
    }

    fn lifecycle_guard(&self) -> Result<std::sync::MutexGuard<'_, ()>, NativeRuntimeError> {
        self.lifecycle
            .lock()
            .map_err(|_| NativeRuntimeError::Agent("credential lifecycle lock poisoned".into()))
    }
}

fn recovered_revoked_descriptor(
    mut descriptor: CredentialDescriptor,
    intent: Option<&ProductCredentialRevocationIntent>,
    fallback_reason: &str,
) -> Result<CredentialDescriptor, NativeRuntimeError> {
    descriptor.credential.revision = descriptor
        .credential
        .revision
        .checked_add(1)
        .ok_or_else(|| NativeRuntimeError::Agent("credential revision overflowed".into()))?;
    descriptor.status = CredentialStatus::Revoked;
    descriptor.revocation = Some(CredentialRevocationInfo {
        revoked_at_unix_ms: intent
            .map(|intent| intent.requested_at_unix_ms)
            .unwrap_or_else(unix_now_ms),
        reason: intent
            .and_then(|intent| intent.reason.clone())
            .or_else(|| Some(fallback_reason.to_owned())),
        independent_revoke_uri: descriptor.independent_revoke_uri.clone(),
    });
    Ok(descriptor)
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn sqlite_table_has_column(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, NativeRuntimeError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(registry_storage_error)?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(registry_storage_error)?;
    for candidate in columns {
        if candidate.map_err(registry_storage_error)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn registry_storage_error(error: rusqlite::Error) -> NativeRuntimeError {
    NativeRuntimeError::Agent(format!("credential registry storage failed: {error}"))
}

fn map_broker_error(err: mutsuki_agent_contracts::AgentError) -> NativeRuntimeError {
    if err.code == CREDENTIAL_UNSUPPORTED_FOR_CUSTOM_RUNTIME {
        NativeRuntimeError::Agent(format!(
            "{CREDENTIAL_UNSUPPORTED_FOR_CUSTOM_RUNTIME}: {}",
            err.message
        ))
    } else {
        NativeRuntimeError::Agent(format!("{}: {}", err.code, err.message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    #[derive(Default)]
    struct RejectingCredentialRegistry;

    impl ProductCredentialRegistry for RejectingCredentialRegistry {
        fn load(&self) -> Result<ProductCredentialRegistryLoad, NativeRuntimeError> {
            Ok(ProductCredentialRegistryLoad::default())
        }

        fn upsert(&self, _record: &ProductCredentialRecord) -> Result<(), NativeRuntimeError> {
            Err(NativeRuntimeError::Agent(
                "credential registry write rejected".into(),
            ))
        }
    }

    struct FailingWriteCredentialRegistry {
        inner: InMemoryProductCredentialRegistry,
        writes: AtomicU64,
        fail_at: u64,
    }

    impl FailingWriteCredentialRegistry {
        fn new(fail_at: u64) -> Self {
            Self {
                inner: InMemoryProductCredentialRegistry::default(),
                writes: AtomicU64::new(0),
                fail_at,
            }
        }
    }

    impl ProductCredentialRegistry for FailingWriteCredentialRegistry {
        fn load(&self) -> Result<ProductCredentialRegistryLoad, NativeRuntimeError> {
            self.inner.load()
        }

        fn upsert(&self, record: &ProductCredentialRecord) -> Result<(), NativeRuntimeError> {
            let write = self.writes.fetch_add(1, Ordering::SeqCst) + 1;
            if write == self.fail_at {
                return Err(NativeRuntimeError::Agent(
                    "injected credential registry failure".into(),
                ));
            }
            self.inner.upsert(record)
        }
    }

    #[derive(Default)]
    struct ToggleDeleteSecretStore {
        inner: InMemorySecretStore,
        reject_delete: AtomicBool,
    }

    impl SecretStore for ToggleDeleteSecretStore {
        fn put(&self, secret_id: &str, material: &str) -> mutsuki_agent_contracts::AgentResult<()> {
            self.inner.put(secret_id, material)
        }

        fn get(&self, secret_id: &str) -> mutsuki_agent_contracts::AgentResult<Option<String>> {
            self.inner.get(secret_id)
        }

        fn delete(&self, secret_id: &str) -> mutsuki_agent_contracts::AgentResult<()> {
            if self.reject_delete.load(Ordering::SeqCst) {
                return Err(mutsuki_agent_contracts::AgentError::new(
                    mutsuki_agent_contracts::CREDENTIAL_UNAVAILABLE,
                    "injected secure storage delete failure",
                ));
            }
            self.inner.delete(secret_id)
        }
    }

    fn sqlite_registry_path() -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "lilia-credential-registry-{}-{}.db",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn openai_and_anthropic_api_keys_login_and_diagnose_independently() {
        let bridge = ProductCredentialBridge::new();
        let openai = bridge
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-0123456789abcdef".into(),
                account_label: Some("openai".into()),
                source: Some("user_api_key".into()),
            })
            .unwrap();
        let anthropic = bridge
            .login(ProductCredentialLoginInput {
                provider_id: ANTHROPIC_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-ant-api03-console-key-0123456789abcdef".into(),
                account_label: None,
                source: Some("anthropic_console".into()),
            })
            .unwrap();
        let health = bridge.health();
        assert!(health.broker_ready);
        assert!(health.has_usable_model_credential);
        assert_eq!(health.active_count, 2);
        assert!(!serde_json::to_string(&openai).unwrap().contains("sk-test"));
        assert!(!serde_json::to_string(&anthropic)
            .unwrap()
            .contains("sk-ant"));
        bridge
            .resolve_for_adapter(&CredentialRef {
                credential_id: openai.credential_id.clone(),
                revision: openai.revision,
            })
            .unwrap();
    }

    #[test]
    fn claude_subscription_credential_is_rejected() {
        let bridge = ProductCredentialBridge::new();
        let err = bridge
            .import_generated_api_key(ProductCredentialImportInput {
                provider_id: ANTHROPIC_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-ant-sid-claude-code-subscription-token".into(),
                account_label: None,
                source: Some("claude_code".into()),
                permissions_summary: None,
                independent_revoke_uri: None,
            })
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(CREDENTIAL_UNSUPPORTED_FOR_CUSTOM_RUNTIME));
        assert!(!bridge.health().has_usable_model_credential);
    }

    #[test]
    fn sqlite_registry_restores_and_retains_revoked_descriptor_without_secret() {
        let path = sqlite_registry_path();
        let secret_material = "sk-test-openai-api-key-0123456789abcdef";
        let secrets = Arc::new(InMemorySecretStore::default());
        let credential = {
            let registry = Arc::new(SqliteProductCredentialRegistry::open(&path).unwrap());
            let bridge =
                ProductCredentialBridge::with_persistence(secrets.clone(), registry.clone())
                    .unwrap();
            let credential = bridge
                .login(ProductCredentialLoginInput {
                    provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                    kind: CredentialKind::ApiKey,
                    secret_material: secret_material.into(),
                    account_label: Some("persistent".into()),
                    source: Some("test-keyring".into()),
                })
                .unwrap();
            let records = registry.load().unwrap().records;
            assert_eq!(records.len(), 1);
            assert!(!serde_json::to_string(&records[0])
                .unwrap()
                .contains(secret_material));
            credential
        };

        {
            let registry = Arc::new(SqliteProductCredentialRegistry::open(&path).unwrap());
            let restored =
                ProductCredentialBridge::with_persistence(secrets.clone(), registry.clone())
                    .unwrap();
            assert_eq!(restored.health().active_count, 1);
            restored
                .resolve_for_adapter(&CredentialRef {
                    credential_id: credential.credential_id.clone(),
                    revision: credential.revision,
                })
                .unwrap();
            let revoked = restored
                .revoke(
                    CredentialRef {
                        credential_id: credential.credential_id.clone(),
                        revision: credential.revision,
                    },
                    Some("test revoke".into()),
                )
                .unwrap();
            assert_eq!(revoked.status, CredentialStatus::Revoked);
            let record = registry.load().unwrap().records.pop().unwrap();
            assert_eq!(record.descriptor.status, CredentialStatus::Revoked);
            assert!(secrets.get(&record.secret_id).unwrap().is_none());
        }

        {
            let registry = Arc::new(SqliteProductCredentialRegistry::open(&path).unwrap());
            let restored = ProductCredentialBridge::with_persistence(secrets, registry).unwrap();
            let health = restored.health();
            assert_eq!(health.credential_count, 1);
            assert_eq!(health.active_count, 0);
            assert_eq!(health.credentials[0].status, CredentialStatus::Revoked);
        }

        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes
            .windows(secret_material.len())
            .any(|window| window == secret_material.as_bytes()));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn registry_failure_rolls_back_new_broker_descriptor_and_secret() {
        let secret_material = "sk-test-openai-api-key-0123456789abcdef";
        let secrets = Arc::new(InMemorySecretStore::default());
        let bridge = ProductCredentialBridge::with_persistence(
            secrets.clone(),
            Arc::new(RejectingCredentialRegistry),
        )
        .unwrap();

        let error = bridge
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: secret_material.into(),
                account_label: None,
                source: None,
            })
            .unwrap_err();

        assert!(error.to_string().contains("registry write rejected"));
        assert!(!error.to_string().contains(secret_material));
        assert_eq!(bridge.health().credential_count, 0);
        assert!(secrets.get("secret-cred-1").unwrap().is_none());
    }

    #[test]
    fn secret_bearing_request_debug_output_is_redacted() {
        let login = ProductCredentialLoginInput {
            provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
            kind: CredentialKind::ApiKey,
            secret_material: "never-log-login-secret".into(),
            account_label: None,
            source: None,
        };
        let imported = ProductCredentialImportInput {
            provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
            kind: CredentialKind::GeneratedApiKey,
            secret_material: "never-log-import-secret".into(),
            account_label: None,
            source: None,
            permissions_summary: None,
            independent_revoke_uri: None,
        };

        let output = format!("{login:?} {imported:?}");
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("never-log-login-secret"));
        assert!(!output.contains("never-log-import-secret"));
    }

    #[test]
    fn missing_secret_degrades_one_credential_without_blocking_restore() {
        let registry = Arc::new(InMemoryProductCredentialRegistry::default());
        let secrets = Arc::new(InMemorySecretStore::default());
        let bridge =
            ProductCredentialBridge::with_persistence(secrets.clone(), registry.clone()).unwrap();
        let credential = bridge
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-missing-secret-0123456789abcdef".into(),
                account_label: None,
                source: None,
            })
            .unwrap();
        let record = registry.load().unwrap().records.pop().unwrap();
        secrets.delete(&record.secret_id).unwrap();
        drop(bridge);

        let restored =
            ProductCredentialBridge::with_persistence(secrets, registry.clone()).unwrap();
        let health = restored.health();
        assert!(health.broker_degraded);
        assert_eq!(health.credential_count, 1);
        assert_eq!(health.active_count, 0);
        assert_eq!(health.credentials[0].status, CredentialStatus::Revoked);
        let repaired = registry.load().unwrap().records.pop().unwrap();
        assert_eq!(repaired.descriptor.status, CredentialStatus::Revoked);
        assert!(repaired.revocation_intent.is_none());
        assert_ne!(repaired.descriptor.credential.revision, credential.revision);
    }

    #[test]
    fn incomplete_revoke_is_replayed_after_final_registry_write_failure() {
        let registry = Arc::new(FailingWriteCredentialRegistry::new(3));
        let secrets = Arc::new(InMemorySecretStore::default());
        let bridge =
            ProductCredentialBridge::with_persistence(secrets.clone(), registry.clone()).unwrap();
        let credential = bridge
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-revoke-saga-0123456789abcdef".into(),
                account_label: None,
                source: None,
            })
            .unwrap();

        let error = bridge
            .revoke(
                CredentialRef {
                    credential_id: credential.credential_id,
                    revision: credential.revision,
                },
                Some("saga test".into()),
            )
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("injected credential registry failure"));
        let pending = registry.inner.load().unwrap().records.pop().unwrap();
        assert!(pending.revocation_intent.is_some());
        drop(bridge);

        let restored =
            ProductCredentialBridge::with_persistence(secrets, registry.clone()).unwrap();
        let health = restored.health();
        assert_eq!(health.active_count, 0);
        assert_eq!(health.credentials[0].status, CredentialStatus::Revoked);
        let recovered = registry.inner.load().unwrap().records.pop().unwrap();
        assert_eq!(recovered.descriptor.status, CredentialStatus::Revoked);
        assert!(recovered.revocation_intent.is_none());
    }

    #[test]
    fn secret_delete_failure_revokes_immediately_and_replays_cleanup_after_restart() {
        let registry = Arc::new(InMemoryProductCredentialRegistry::default());
        let secrets = Arc::new(ToggleDeleteSecretStore::default());
        let bridge =
            ProductCredentialBridge::with_persistence(secrets.clone(), registry.clone()).unwrap();
        let credential = bridge
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-delete-recovery-0123456789abcdef".into(),
                account_label: None,
                source: None,
            })
            .unwrap();
        let secret_id = registry.load().unwrap().records[0].secret_id.clone();
        secrets.reject_delete.store(true, Ordering::SeqCst);

        let error = bridge
            .revoke(
                CredentialRef {
                    credential_id: credential.credential_id.clone(),
                    revision: credential.revision,
                },
                Some("delete recovery test".into()),
            )
            .unwrap_err();
        assert!(error.to_string().contains("delete failure"));
        let health = bridge.health();
        assert!(health.broker_degraded);
        assert_eq!(health.active_count, 0);
        assert_eq!(health.credentials[0].status, CredentialStatus::Revoked);
        assert_eq!(health.credentials[0].revision, credential.revision + 1);
        assert!(secrets.get(&secret_id).unwrap().is_some());
        let pending = registry.load().unwrap().records.pop().unwrap();
        assert_eq!(pending.descriptor.status, CredentialStatus::Revoked);
        assert!(pending.revocation_intent.is_some());
        drop(bridge);

        secrets.reject_delete.store(false, Ordering::SeqCst);
        let restored =
            ProductCredentialBridge::with_persistence(secrets.clone(), registry.clone()).unwrap();
        let health = restored.health();
        assert!(!health.broker_degraded);
        assert_eq!(health.credentials[0].status, CredentialStatus::Revoked);
        assert!(secrets.get(&secret_id).unwrap().is_none());
        let recovered = registry.load().unwrap().records.pop().unwrap();
        assert_eq!(recovered.descriptor.status, CredentialStatus::Revoked);
        assert!(recovered.revocation_intent.is_none());
    }

    #[test]
    fn corrupt_registry_row_is_isolated_without_hiding_healthy_credentials() {
        let path = sqlite_registry_path();
        let secrets = Arc::new(InMemorySecretStore::default());
        {
            let registry = Arc::new(SqliteProductCredentialRegistry::open(&path).unwrap());
            let bridge =
                ProductCredentialBridge::with_persistence(secrets.clone(), registry).unwrap();
            bridge
                .login(ProductCredentialLoginInput {
                    provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                    kind: CredentialKind::ApiKey,
                    secret_material: "sk-test-healthy-registry-row-0123456789abcdef".into(),
                    account_label: None,
                    source: None,
                })
                .unwrap();
        }
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute(
                    r#"INSERT INTO product_credential_registry
                       (credential_id, secret_id, descriptor_json, revocation_intent_json)
                       VALUES ('broken-row', 'secret-broken-row', '{', NULL)"#,
                    [],
                )
                .unwrap();
        }

        {
            let registry = Arc::new(SqliteProductCredentialRegistry::open(&path).unwrap());
            let restored = ProductCredentialBridge::with_persistence(secrets, registry).unwrap();
            let health = restored.health();
            assert!(health.broker_degraded);
            assert_eq!(health.active_count, 1);
            assert_eq!(health.recovery_issues.len(), 1);
            assert_eq!(
                health.recovery_issues[0].credential_id.as_deref(),
                Some("broken-row")
            );
        }
        std::fs::remove_file(path).unwrap();
    }
}
