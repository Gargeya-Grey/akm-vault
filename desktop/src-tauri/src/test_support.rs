use akm_core::{KeyDisclosure, Vault};

/// A stored secret, read through a recorded disclosure request.
pub(crate) fn secret_of(vault: &Vault, id: &str) -> String {
    let key = vault.get_by_id(id).unwrap();
    let proof = vault
        .disclose_key(KeyDisclosure::Reveal, &key, Some("test"))
        .unwrap();
    vault.secret(id, &proof).unwrap().to_string()
}

pub(crate) fn secret_titled(vault: &Vault, title: &str) -> String {
    secret_of(vault, &vault.find(title).unwrap().id)
}

/// The variables of one scope with values, through a recorded export request.
pub(crate) fn env_of(vault: &Vault, project: &str, environment: &str) -> Vec<(String, String)> {
    let plan = vault
        .plan_env(Some(project), Some(environment), false)
        .unwrap();
    let proof = vault
        .disclose_env(akm_core::EnvDisclosure::Export, &plan, Some("test"))
        .unwrap();
    vault.env_vars(&plan, &proof).unwrap()
}
