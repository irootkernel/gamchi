//! Resolve spawn `model` and `effort` from parent fields, home YAML, then built-ins.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Built-in model when every layer omits the field.
pub const BUILTIN_MODEL: &str = "grok-4.6";
/// Built-in effort when every layer omits the field.
pub const BUILTIN_EFFORT: &str = "high";
/// Pre-EPIC-006 stub alias. Always becomes [`BUILTIN_MODEL`].
pub const GROK_ALIAS: &str = "grok";

const CONFIG_NAME: &str = "config.yaml";
const KEY_MODEL: &str = "default_model";
const KEY_EFFORT: &str = "default_effort";

/// Optional home-config overrides. `None` means the key was omitted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HomeDefaults {
    pub default_model: Option<String>,
    pub default_effort: Option<String>,
}

/// Why model/effort resolution refused the spawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    InvalidConfig { reason: String },
    ModelMismatch { requested: String, frozen: String },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig { reason } => write!(f, "INVALID_CONFIG: {reason}"),
            Self::ModelMismatch { requested, frozen } => {
                write!(
                    f,
                    "INVALID_CONFIG: follow-up model {requested} is not thread model {frozen}"
                )
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// Read `<home>/config.yaml`. Missing file is both keys omitted.
pub fn load_home_defaults(home: &Path) -> Result<HomeDefaults, ResolveError> {
    let path = home.join(CONFIG_NAME);
    let raw = match fs::read_to_string(&path) {
        Ok(body) => body,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(HomeDefaults::default());
        }
        Err(err) => {
            return Err(ResolveError::InvalidConfig {
                reason: format!("cannot read {CONFIG_NAME}: {err}"),
            });
        }
    };
    parse_home_defaults(&raw)
}

fn parse_home_defaults(raw: &str) -> Result<HomeDefaults, ResolveError> {
    let mut defaults = HomeDefaults::default();
    let mut seen = BTreeSet::new();
    for (idx, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "---" {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') || trimmed.starts_with('-') {
            return Err(ResolveError::InvalidConfig {
                reason: format!("malformed {CONFIG_NAME} line {}", idx + 1),
            });
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            return Err(ResolveError::InvalidConfig {
                reason: format!("malformed {CONFIG_NAME} line {}", idx + 1),
            });
        };
        let key = key.trim();
        if !seen.insert(key.to_string()) {
            return Err(ResolveError::InvalidConfig {
                reason: format!("duplicate key {key} in {CONFIG_NAME}"),
            });
        }
        let value = unquote(value.trim());
        match key {
            KEY_MODEL => defaults.default_model = Some(value),
            KEY_EFFORT => defaults.default_effort = Some(value),
            _ => {
                return Err(ResolveError::InvalidConfig {
                    reason: format!("unknown key {key} in {CONFIG_NAME}"),
                });
            }
        }
    }
    Ok(defaults)
}

fn unquote(value: &str) -> String {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

/// `None` is omitted. `Some` is a parent-sent value, including blank.
pub fn present_field(raw: &str) -> Option<&str> {
    if raw.is_empty() {
        None
    } else {
        Some(raw)
    }
}

/// Normalize a present model or effort. Blank is `INVALID_CONFIG`.
pub fn normalize_present(raw: &str) -> Result<String, ResolveError> {
    if raw.trim().is_empty() {
        return Err(ResolveError::InvalidConfig {
            reason: "blank model or effort".to_string(),
        });
    }
    let trimmed = raw.trim();
    if trimmed == GROK_ALIAS {
        return Ok(BUILTIN_MODEL.to_string());
    }
    Ok(trimmed.to_string())
}

/// First present non-blank layer wins, then `builtin`.
pub fn resolve_layer(
    explicit: Option<&str>,
    config: Option<&str>,
    builtin: &str,
) -> Result<String, ResolveError> {
    if let Some(value) = explicit {
        return normalize_present(value);
    }
    if let Some(value) = config {
        return normalize_present(value);
    }
    Ok(builtin.to_string())
}

/// First-turn model/effort from parent fields and home config.
pub fn resolve_first_turn(
    model: &str,
    effort: &str,
    defaults: &HomeDefaults,
) -> Result<(String, String), ResolveError> {
    let model = resolve_layer(
        present_field(model),
        defaults.default_model.as_deref(),
        BUILTIN_MODEL,
    )?;
    let effort = resolve_layer(
        present_field(effort),
        defaults.default_effort.as_deref(),
        BUILTIN_EFFORT,
    )?;
    Ok((model, effort))
}

/// Follow-up: freeze model; omitted effort uses previous non-blank, else config, else high.
pub fn resolve_follow_up(
    requested_model: &str,
    requested_effort: &str,
    frozen_model: &str,
    previous_effort: &str,
    defaults: &HomeDefaults,
) -> Result<(String, String), ResolveError> {
    let frozen = normalize_present(frozen_model)?;
    let model = if let Some(requested) = present_field(requested_model) {
        let requested = normalize_present(requested)?;
        if requested != frozen {
            return Err(ResolveError::ModelMismatch { requested, frozen });
        }
        requested
    } else {
        frozen
    };
    let effort = if let Some(requested) = present_field(requested_effort) {
        normalize_present(requested)?
    } else if !previous_effort.is_empty() {
        previous_effort.to_string()
    } else {
        resolve_layer(None, defaults.default_effort.as_deref(), BUILTIN_EFFORT)?
    };
    Ok((model, effort))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn write_config(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join(CONFIG_NAME);
        fs::write(&path, body).expect("write config");
        dir.to_path_buf()
    }

    #[test]
    fn missing_file_skips_both_keys() {
        let dir = tempfile::tempdir().expect("home");
        let got = load_home_defaults(dir.path()).expect("load");
        assert_eq!(got, HomeDefaults::default());
    }

    #[test]
    fn absent_key_skips_and_blank_refuses() {
        let dir = tempfile::tempdir().expect("home");
        write_config(dir.path(), "default_model: grok-4.6\n");
        let got = load_home_defaults(dir.path()).expect("load");
        assert_eq!(got.default_model.as_deref(), Some("grok-4.6"));
        assert_eq!(got.default_effort, None);
        write_config(dir.path(), "default_effort:   \n");
        let got = load_home_defaults(dir.path()).expect("load");
        let err = resolve_first_turn("", "", &got).expect_err("blank");
        match err {
            ResolveError::InvalidConfig { reason } => {
                assert!(reason.contains("blank"), "{reason}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unknown_key_and_malformed_refuse() {
        let dir = tempfile::tempdir().expect("home");
        write_config(dir.path(), "model: grok-4.6\n");
        load_home_defaults(dir.path()).expect_err("unknown");
        write_config(dir.path(), "not yaml\n");
        load_home_defaults(dir.path()).expect_err("malformed");
    }

    #[test]
    fn explicit_wins_config_wins_builtin() {
        let defaults = HomeDefaults {
            default_model: Some("from-config".into()),
            default_effort: Some("low".into()),
        };
        let (model, effort) = resolve_first_turn("", "", &defaults).expect("config");
        assert_eq!(model, "from-config");
        assert_eq!(effort, "low");
        let (model, effort) =
            resolve_first_turn("parent-model", "xhigh", &defaults).expect("parent");
        assert_eq!(model, "parent-model");
        assert_eq!(effort, "xhigh");
        let (model, effort) =
            resolve_first_turn("", "", &HomeDefaults::default()).expect("builtin");
        assert_eq!(model, BUILTIN_MODEL);
        assert_eq!(effort, BUILTIN_EFFORT);
    }

    #[test]
    fn grok_alias_normalizes() {
        assert_eq!(normalize_present("grok").unwrap(), BUILTIN_MODEL);
        let (model, _) = resolve_first_turn("grok", "high", &HomeDefaults::default()).unwrap();
        assert_eq!(model, BUILTIN_MODEL);
    }

    #[test]
    fn empty_stored_effort_uses_home_or_high() {
        let defaults = HomeDefaults {
            default_effort: Some("low".into()),
            ..HomeDefaults::default()
        };
        let (_, effort) =
            resolve_follow_up("", "", "grok-4.6", "", &defaults).expect("empty stored");
        assert_eq!(effort, "low");
        let (_, effort) =
            resolve_follow_up("", "", "grok-4.6", "", &HomeDefaults::default()).expect("builtin");
        assert_eq!(effort, BUILTIN_EFFORT);
        let (_, effort) =
            resolve_follow_up("", "", "grok-4.6", "medium", &defaults).expect("previous");
        assert_eq!(effort, "medium");
    }

    #[test]
    fn follow_up_model_mismatch_refuses_after_alias() {
        let err = resolve_follow_up("other", "", "grok", "", &HomeDefaults::default())
            .expect_err("mismatch");
        match err {
            ResolveError::ModelMismatch { requested, frozen } => {
                assert_eq!(requested, "other");
                assert_eq!(frozen, BUILTIN_MODEL);
            }
            other => panic!("{other:?}"),
        }
        let (model, _) =
            resolve_follow_up("grok", "", "grok-4.6", "high", &HomeDefaults::default()).unwrap();
        assert_eq!(model, BUILTIN_MODEL);
    }
}
