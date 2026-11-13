use std::ffi::OsStr;
use std::ops::Deref;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use akm_core::model::{name_class, NameClass};
use akm_core::{Error, Result};
use zeroize::Zeroize;

pub struct Secrets(pub Vec<(String, String)>);

impl Deref for Secrets {
    type Target = [(String, String)];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for Secrets {
    fn drop(&mut self) {
        for (_, value) in &mut self.0 {
            value.zeroize();
        }
    }
}

pub fn prepare(program: &str, args: &[String]) -> Result<Command> {
    let mut command = Command::new(resolve(program)?);
    command.args(args);
    for (name, _) in std::env::vars_os() {
        if is_akm_name(&name) {
            command.env_remove(name);
        }
    }
    Ok(command)
}

pub fn validate(envs: &[(String, String)]) -> Result<()> {
    validate_names(envs.iter().map(|(name, _)| name.as_str()))
}

pub fn validate_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<()> {
    for name in names {
        if let NameClass::Reserved(class) = name_class(name) {
            return Err(Error::Invalid(format!(
                "a selected key uses the reserved variable {name:?}: {}",
                class.reason()
            )));
        }
    }
    Ok(())
}

pub fn inject(command: &mut Command, envs: &[(String, String)]) -> Result<()> {
    validate(envs)?;
    command.envs(envs.iter().map(|(name, value)| (name, value)));
    Ok(())
}

/// The desktop test `the_akm_name_rule_mirrors_the_one_of_the_command_line` pins this text,
/// and the test below holds it to `name_class`.
fn is_akm_name(name: &OsStr) -> bool {
    name.to_string_lossy()
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("AKM_"))
}

fn resolve(program: &str) -> Result<PathBuf> {
    let path = Path::new(program);
    let bare = !program.is_empty()
        && path.components().count() == 1
        && matches!(path.components().next(), Some(Component::Normal(_)));

    if !bare {
        return executable(path).ok_or_else(command_not_found);
    }

    if let Some(paths) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&paths) {
            // Only an explicit PATH entry such as "." opts into the working directory.
            if directory.as_os_str().is_empty() {
                continue;
            }
            if let Some(path) = executable(&directory.join(path)) {
                return Ok(path);
            }
        }
    }
    Err(command_not_found())
}

fn command_not_found() -> Error {
    Error::Invalid(
        "command not found; use an explicit executable path or add its directory to PATH".into(),
    )
}

#[cfg(windows)]
fn executable(path: &Path) -> Option<PathBuf> {
    if path.extension().is_some() {
        return path.is_file().then(|| path.canonicalize().ok()).flatten();
    }
    let extensions = std::env::var_os("PATHEXT").unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into());
    for extension in extensions.to_string_lossy().split(';') {
        let extension = extension.trim();
        if !extension.starts_with('.')
            || !extension[1..]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric())
        {
            continue;
        }
        let mut candidate = path.as_os_str().to_os_string();
        candidate.push(extension);
        let candidate = PathBuf::from(candidate);
        if candidate.is_file() {
            return candidate.canonicalize().ok();
        }
    }
    None
}

#[cfg(not(windows))]
fn executable(path: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = path.metadata().ok()?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return None;
    }
    path.canonicalize().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use akm_core::model::ReservedClass;

    /// The inherited variables that a child loses are the AkmInternal class of `name_class`.
    #[test]
    fn the_akm_name_rule_is_the_akm_internal_class_of_name_class() {
        for name in [
            "AKM_PASSWORD",
            "akm_password",
            "Akm_",
            "AKM_VAULT",
            "AKM",
            "AKMX",
            "XAKM_Y",
            "PATH",
            "HOST_CC",
            "",
        ] {
            assert_eq!(
                is_akm_name(OsStr::new(name)),
                name_class(name) == NameClass::Reserved(ReservedClass::AkmInternal),
                "{name:?}"
            );
        }
    }
}
