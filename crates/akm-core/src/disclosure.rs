//! The capability that lets a secret leave the vault (unit 1h, design D9).
//!
//! A [`Disclosure`] exists only after the journal recorded the request. Only
//! [`crate::Vault::disclose_key`] and [`crate::Vault::disclose_env`] build one, and they
//! build it after the append succeeded, so a journal failure leaves no proof and no
//! secret can be read.
//!
//! A proof is bound to the vault session that recorded it, to the state of that vault, and
//! to one kind of action (a key or a planned scope). A reader uses it up: the second use is
//! refused. A refused presentation does not use it up. A session records in the journal that
//! its own evidence selects, and stale evidence never rebinds a journal (unit 1b, D10), so a
//! proof bound to the session is bound to its journal too.
//!
//! ```compile_fail
//! // A key has no secret field to read.
//! fn read(key: &akm_core::ApiKey) -> &str { &key.secret }
//! ```
//!
//! ```compile_fail,E0624
//! // A proof cannot be built outside the vault.
//! let _ = akm_core::Disclosure::key(
//!     Default::default(),
//!     akm_core::KeyDisclosure::Reveal,
//!     "id".to_string(),
//! );
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

use std::sync::atomic::{AtomicBool, Ordering};

use uuid::Uuid;

use crate::error::{Error, Result};

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

/// The vault session, and the state of it, that a proof or a plan was made for. A session
/// is one `Vault` value. Its state changes with every save and every reload.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Stamp {
    session: Uuid,
    state: u64,
}

impl Stamp {
    pub(crate) fn new(session: Uuid, state: u64) -> Self {
        Self { session, state }
    }

    fn proof_is_current(self, current: Stamp) -> Result<()> {
        if self.session != current.session {
            return Err(Error::Invalid(
                "this disclosure record was issued for another vault session".into(),
            ));
        }
        if self.state != current.state {
            return Err(Error::Invalid(
                "this disclosure record was issued before the vault changed".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn plan_is_current(self, current: Stamp) -> Result<()> {
        if self == current {
            return Ok(());
        }
        Err(Error::Invalid(
            "this selection was planned for another vault session or an earlier state of it".into(),
        ))
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
pub struct Disclosure {
    stamp: Stamp,
    covers: Covers,
    used: AtomicBool,
}

impl Disclosure {
    pub(crate) fn key(stamp: Stamp, action: KeyDisclosure, id: String) -> Self {
        Self::new(stamp, Covers::Key { action, id })
    }

    pub(crate) fn env(stamp: Stamp, action: EnvDisclosure, ids: Vec<String>) -> Self {
        Self::new(stamp, Covers::Env { action, ids })
    }

    fn new(stamp: Stamp, covers: Covers) -> Self {
        Self {
            stamp,
            covers,
            used: AtomicBool::new(false),
        }
    }

    /// The journal action that this proof was recorded under.
    pub fn action(&self) -> &'static str {
        match &self.covers {
            Covers::Key { action, .. } => action.action(),
            Covers::Env { action, .. } => action.action(),
        }
    }

    /// The only way a reader accepts a proof for one key: the proof must be current, be a
    /// key proof for `id` and be unused. Success uses it up.
    pub(crate) fn redeem_key(&self, current: Stamp, id: &str) -> Result<()> {
        self.stamp.proof_is_current(current)?;
        if !matches!(&self.covers, Covers::Key { id: covered, .. } if covered == id) {
            return Err(Error::Invalid(
                "this disclosure record does not cover that key".into(),
            ));
        }
        self.use_up()
    }

    /// The only way a reader accepts a proof for a plan: the plan and the proof must be
    /// current, the proof must be a scope proof for exactly the plan's keys, and unused.
    /// Success uses it up.
    pub(crate) fn redeem_env(&self, current: Stamp, plan: &EnvPlan) -> Result<()> {
        plan.stamp.plan_is_current(current)?;
        self.stamp.proof_is_current(current)?;
        if !matches!(&self.covers, Covers::Env { ids, .. } if *ids == plan.key_ids()) {
            return Err(Error::Invalid(
                "this disclosure record does not cover that selection".into(),
            ));
        }
        self.use_up()
    }

    fn use_up(&self) -> Result<()> {
        if self.used.swap(true, Ordering::SeqCst) {
            return Err(Error::Invalid(
                "this disclosure record was already used".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlannedVar {
    key_id: String,
    name: String,
}

/// The variables a scope would disclose: names and key IDs, never values. A plan belongs
/// to the vault session, and the state of it, that made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvPlan {
    stamp: Stamp,
    vars: Vec<PlannedVar>,
}

impl EnvPlan {
    pub(crate) fn new(stamp: Stamp, vars: Vec<(String, String)>) -> Self {
        Self {
            stamp,
            vars: vars
                .into_iter()
                .map(|(key_id, name)| PlannedVar { key_id, name })
                .collect(),
        }
    }

    pub(crate) fn stamp(&self) -> Stamp {
        self.stamp
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
