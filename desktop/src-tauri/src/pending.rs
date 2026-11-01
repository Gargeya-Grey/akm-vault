use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{error::CommandError, policy::Ticket};

pub struct PendingImport {
    token: String,
    ticket: Ticket,
    revision: u64,
    fingerprint: [u8; 32],
    pub text: Zeroizing<String>,
    pub project: Option<String>,
    pub environment: Option<String>,
}

#[derive(Default)]
pub struct PendingImports(Option<PendingImport>);

impl PendingImports {
    pub fn clear(&mut self) {
        self.0 = None;
    }

    pub fn set(
        &mut self,
        ticket: Ticket,
        revision: u64,
        text: Zeroizing<String>,
        project: Option<String>,
        environment: Option<String>,
    ) -> String {
        let token = Uuid::new_v4().to_string();
        self.0 = Some(PendingImport {
            token: token.clone(),
            ticket,
            revision,
            fingerprint: fingerprint(&text, project.as_deref(), environment.as_deref()),
            text,
            project,
            environment,
        });
        token
    }

    pub fn take(
        &mut self,
        token: &str,
        ticket: Ticket,
        revision: u64,
    ) -> Result<PendingImport, CommandError> {
        let pending = self.0.take().ok_or_else(stale)?;
        if pending.token != token
            || pending.ticket != ticket
            || pending.revision != revision
            || pending.fingerprint
                != fingerprint(
                    &pending.text,
                    pending.project.as_deref(),
                    pending.environment.as_deref(),
                )
        {
            return Err(stale());
        }
        Ok(pending)
    }
}

fn stale() -> CommandError {
    CommandError::new(
        "stalePreview",
        "This import preview expired or was already used. Preview the import again.",
    )
}

fn fingerprint(text: &str, project: Option<&str>, environment: Option<&str>) -> [u8; 32] {
    let mut hash = Sha256::new();
    for value in [Some(text), project, environment] {
        hash.update([u8::from(value.is_some())]);
        let bytes = value.unwrap_or("").as_bytes();
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    hash.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preview(pending: &mut PendingImports) -> String {
        pending.set(
            Ticket(3),
            7,
            Zeroizing::new("TOKEN=fake".into()),
            Some("app".into()),
            Some("dev".into()),
        )
    }

    #[test]
    fn preview_token_is_single_use_and_bound_to_its_session_and_revision() {
        let mut pending = PendingImports::default();
        let token = preview(&mut pending);
        let accepted = pending.take(&token, Ticket(3), 7).unwrap();
        assert_eq!(accepted.text.as_str(), "TOKEN=fake");
        assert_eq!(accepted.project.as_deref(), Some("app"));
        assert!(pending.take(&token, Ticket(3), 7).is_err());
        let token = preview(&mut pending);
        assert!(pending.take(&token, Ticket(4), 7).is_err());
        let token = preview(&mut pending);
        assert!(pending.take(&token, Ticket(3), 8).is_err());
    }

    #[test]
    fn lock_reload_upgrade_and_replacing_preview_discard_the_pending_input() {
        let mut pending = PendingImports::default();
        let old = preview(&mut pending);
        preview(&mut pending);
        assert!(pending.take(&old, Ticket(3), 7).is_err());
        let token = preview(&mut pending);
        pending.clear();
        assert!(pending.take(&token, Ticket(3), 7).is_err());
    }
}
