//! The capability that lets a secret leave the vault (unit 1h, design D9).
//!
//! A [`Disclosure`] exists only after the journal recorded the request. Only
//! [`crate::Vault::disclose_key`] and [`crate::Vault::disclose_env`] build one, and they
//! build it after the append succeeded, so a journal failure leaves no proof and no
//! secret can be read.
//!
//! ```compile_fail
//! // A key has no secret field to read.
//! fn read(key: &akm_core::ApiKey) -> &str { &key.secret }
//! ```
//!
//! ```compile_fail
//! // A proof cannot be built outside the vault.
//! let _ = akm_core::Disclosure::key(akm_core::KeyDisclosure::Reveal, "id".to_string());
//! ```
//!
//! ```compile_fail
//! // Environment values need a proof.
//! fn read(vault: &akm_core::Vault, plan: &akm_core::EnvPlan) {
//!     let _ = vault.env_vars(plan);
//! }
//! ```
//!
//! ```compile_fail
//! // A secret has no text accessor outside the crate.
//! fn read(secret: &akm_core::Secret) -> &str { secret.as_str() }
//! ```
//!
//! The controls below compile, so each failure above comes from the one line that differs.
//!
//! ```
//! fn read(vault: &akm_core::Vault, plan: &akm_core::EnvPlan, proof: &akm_core::Disclosure) {
//!     let _ = vault.env_vars(plan, proof);
//! }
//! fn id(key: &akm_core::ApiKey) -> &str { &key.id }
//! fn proof(vault: &akm_core::Vault, key: &akm_core::ApiKey) {
//!     let _ = vault.disclose_key(akm_core::KeyDisclosure::Reveal, key, None);
//! }
//! ```

/// The two ways one key's secret can leave the vault. The names are the journal actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyDisclosure {
    Reveal,
    Copy,
}

impl KeyDisclosure {
    pub fn action(self) -> &'static str {
        match self {
            Self::Reveal => "reveal-requested",
            Self::Copy => "copy-requested",
        }
    }
}

/// The two ways a scope's secrets can leave the vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvDisclosure {
    Export,
    Run,
}

impl EnvDisclosure {
    pub fn action(self) -> &'static str {
        match self {
            Self::Export => "export-requested",
            Self::Run => "run-requested",
        }
    }
}

#[derive(Debug)]
enum Covers {
    Key {
        action: KeyDisclosure,
        id: String,
    },
    Env {
        action: EnvDisclosure,
        ids: Vec<String>,
    },
}

/// Proof that the journal recorded one disclosure request, for the keys it names.
#[derive(Debug)]
pub struct Disclosure(Covers);

impl Disclosure {
    pub(crate) fn key(action: KeyDisclosure, id: String) -> Self {
        Self(Covers::Key { action, id })
    }

    pub(crate) fn env(action: EnvDisclosure, ids: Vec<String>) -> Self {
        Self(Covers::Env { action, ids })
    }

    /// The journal action that this proof was recorded under.
    pub fn action(&self) -> &'static str {
        match &self.0 {
            Covers::Key { action, .. } => action.action(),
            Covers::Env { action, .. } => action.action(),
        }
    }

    pub(crate) fn covers_key(&self, id: &str) -> bool {
        matches!(&self.0, Covers::Key { id: covered, .. } if covered == id)
    }

    pub(crate) fn covers_env(&self, plan: &EnvPlan) -> bool {
        matches!(&self.0, Covers::Env { ids, .. } if *ids == plan.key_ids())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlannedVar {
    key_id: String,
    name: String,
}

/// The variables a scope would disclose: names and key IDs, never values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvPlan {
    vars: Vec<PlannedVar>,
}

impl EnvPlan {
    pub(crate) fn new(vars: Vec<(String, String)>) -> Self {
        Self {
            vars: vars
                .into_iter()
                .map(|(key_id, name)| PlannedVar { key_id, name })
                .collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.vars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }

    /// The variable names, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.vars.iter().map(|var| var.name.as_str())
    }

    pub(crate) fn key_ids(&self) -> Vec<String> {
        self.vars.iter().map(|var| var.key_id.clone()).collect()
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.vars
            .iter()
            .map(|var| (var.key_id.as_str(), var.name.as_str()))
    }
}
