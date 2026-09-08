//! Ledger home resolution. Facades pass `--home`; this module does not parse CLI.

use std::fmt;
use std::path::{Path, PathBuf};

/// Environment override. Absolute path required when set.
pub const ENV_HOME: &str = "SAMCHI_FOR_GROK_HOME";

/// Directory name under the user home when neither `--home` nor the env is set.
pub const DEFAULT_DIR_NAME: &str = ".samchi-for-grok";

/// Failed home resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeError {
    pub reason: String,
}

impl fmt::Display for HomeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid home: {}", self.reason)
    }
}

impl std::error::Error for HomeError {}

/// Resolve the ledger root.
///
/// Order: `explicit` (`--home`), then `env_home` (`SAMCHI_FOR_GROK_HOME`),
/// then `user_home` / `.samchi-for-grok`. `--home` and the env value must be
/// absolute. Empty env is treated as unset.
pub fn resolve_home(
    explicit: Option<&Path>,
    env_home: Option<&Path>,
    user_home: &Path,
) -> Result<PathBuf, HomeError> {
    if let Some(path) = explicit {
        return require_absolute(path, "--home");
    }
    if let Some(path) = env_home {
        if !path.as_os_str().is_empty() {
            return require_absolute(path, ENV_HOME);
        }
    }
    if !user_home.is_absolute() {
        return Err(HomeError {
            reason: "user home must be absolute".to_string(),
        });
    }
    Ok(user_home.join(DEFAULT_DIR_NAME))
}

/// Read `SAMCHI_FOR_GROK_HOME` and `HOME` from the process environment.
pub fn resolve_home_from_os(explicit: Option<&Path>) -> Result<PathBuf, HomeError> {
    let env_home = std::env::var_os(ENV_HOME).map(PathBuf::from);
    let user_home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(HomeError {
            reason: "HOME is unset".to_string(),
        })?;
    resolve_home(explicit, env_home.as_deref(), &user_home)
}

fn require_absolute(path: &Path, label: &str) -> Result<PathBuf, HomeError> {
    if path.as_os_str().is_empty() {
        return Err(HomeError {
            reason: format!("{label} is empty"),
        });
    }
    if !path.is_absolute() {
        return Err(HomeError {
            reason: format!("{label} must be an absolute path"),
        });
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn explicit_wins_over_env_and_user() {
        let got = resolve_home(
            Some(Path::new("/explicit/home")),
            Some(Path::new("/env/home")),
            Path::new("/user"),
        )
        .unwrap();
        assert_eq!(got, PathBuf::from("/explicit/home"));
    }

    #[test]
    fn env_wins_over_user() {
        let got = resolve_home(None, Some(Path::new("/env/home")), Path::new("/user")).unwrap();
        assert_eq!(got, PathBuf::from("/env/home"));
    }

    #[test]
    fn default_is_dot_dir_under_user_home() {
        let got = resolve_home(None, None, Path::new("/Users/sam")).unwrap();
        assert_eq!(got, PathBuf::from("/Users/sam/.samchi-for-grok"));
        assert_ne!(got, PathBuf::from("/Users/sam"));
    }

    #[test]
    fn empty_env_falls_through_to_default() {
        let got = resolve_home(None, Some(Path::new("")), Path::new("/Users/sam")).unwrap();
        assert_eq!(got, PathBuf::from("/Users/sam/.samchi-for-grok"));
    }

    #[test]
    fn relative_explicit_is_rejected() {
        let err = resolve_home(Some(Path::new("rel/home")), None, Path::new("/user")).unwrap_err();
        assert!(err.reason.contains("--home"), "reason {:?}", err.reason);
    }

    #[test]
    fn relative_env_is_rejected() {
        let err = resolve_home(None, Some(Path::new("rel-env")), Path::new("/user")).unwrap_err();
        assert!(err.reason.contains(ENV_HOME), "reason {:?}", err.reason);
    }

    #[test]
    fn relative_user_home_is_rejected() {
        let err = resolve_home(None, None, Path::new("not-absolute")).unwrap_err();
        assert!(err.reason.contains("user home"), "reason {:?}", err.reason);
    }
}
