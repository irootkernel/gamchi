//! Map core spawn fields onto `grok` argv. Fail closed when Grok cannot enforce.

use samchi_core::source_wire::{ApprovalPolicy, ThreadSandbox};
use std::path::{Path, PathBuf};

/// Optional subset fields the parent must not send unless the adapter implements them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtraSpawnFields {
    pub writable_roots: Option<Vec<String>>,
    pub network_access: Option<bool>,
    pub exclude_slash_tmp: Option<bool>,
    pub exclude_tmpdir_env_var: Option<bool>,
}

impl ExtraSpawnFields {
    /// First present extra field, if any.
    pub fn first_set(&self) -> Option<&'static str> {
        if self.writable_roots.is_some() {
            return Some("writableRoots");
        }
        if self.network_access.is_some() {
            return Some("networkAccess");
        }
        if self.exclude_slash_tmp.is_some() {
            return Some("excludeSlashTmp");
        }
        if self.exclude_tmpdir_env_var.is_some() {
            return Some("excludeTmpdirEnvVar");
        }
        None
    }
}

/// Parent spawn request before argv is built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchRequest<'a> {
    pub program: &'a Path,
    pub cwd: &'a Path,
    pub approval: ApprovalPolicy,
    pub sandbox: ThreadSandbox,
    pub extra: ExtraSpawnFields,
}

/// Parent-owned `grok agent stdio` argv.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// Why spawn was refused before a child started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    Unenforceable { field: String, reason: String },
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unenforceable { field, reason } => {
                write!(f, "unenforceable {field}: {reason}")
            }
        }
    }
}

impl std::error::Error for LaunchError {}

/// Map spawn fields to argv, or refuse before the child starts.
///
/// `approvalPolicy=never` → `--always-approve`. `sandbox=workspace-write` →
/// top-level `grok --sandbox workspace` (not `grok agent --sandbox`).
pub fn plan_launch(req: &LaunchRequest<'_>) -> Result<LaunchPlan, LaunchError> {
    if let Some(field) = req.extra.first_set() {
        return Err(LaunchError::Unenforceable {
            field: field.to_string(),
            reason: "adapter cannot implement this subset field".to_string(),
        });
    }
    match req.approval {
        ApprovalPolicy::Never => {}
        ApprovalPolicy::Untrusted | ApprovalPolicy::OnRequest => {
            return Err(LaunchError::Unenforceable {
                field: "approvalPolicy".to_string(),
                reason: "untrusted/on-request wait for TASK-013 grok_respond".to_string(),
            });
        }
    }
    match req.sandbox {
        ThreadSandbox::WorkspaceWrite => {}
        ThreadSandbox::ReadOnly => {
            return Err(LaunchError::Unenforceable {
                field: "sandbox".to_string(),
                reason: "no verified Grok profile that denies workspace writes".to_string(),
            });
        }
    }
    if !req.cwd.is_absolute() {
        return Err(LaunchError::Unenforceable {
            field: "cwd".to_string(),
            reason: "cwd must be an absolute path".to_string(),
        });
    }

    Ok(LaunchPlan {
        program: req.program.to_path_buf(),
        args: vec![
            "--cwd".to_string(),
            req.cwd.display().to_string(),
            "--sandbox".to_string(),
            "workspace".to_string(),
            "agent".to_string(),
            "--no-leader".to_string(),
            "--always-approve".to_string(),
            "stdio".to_string(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req<'a>(
        program: &'a Path,
        cwd: &'a Path,
        approval: ApprovalPolicy,
        sandbox: ThreadSandbox,
        extra: ExtraSpawnFields,
    ) -> LaunchRequest<'a> {
        LaunchRequest {
            program,
            cwd,
            approval,
            sandbox,
            extra,
        }
    }

    #[test]
    fn never_workspace_write_maps_to_sandbox_then_agent() {
        let program = Path::new("grok");
        let cwd = Path::new("/tmp/samchi-launch");
        let plan = plan_launch(&req(
            program,
            cwd,
            ApprovalPolicy::Never,
            ThreadSandbox::WorkspaceWrite,
            ExtraSpawnFields::default(),
        ))
        .expect("launch");
        assert_eq!(plan.program, program);
        assert_eq!(
            plan.args,
            vec![
                "--cwd",
                "/tmp/samchi-launch",
                "--sandbox",
                "workspace",
                "agent",
                "--no-leader",
                "--always-approve",
                "stdio",
            ]
        );
        let sandbox_at = plan.args.iter().position(|a| a == "--sandbox").unwrap();
        let agent_at = plan.args.iter().position(|a| a == "agent").unwrap();
        assert!(
            sandbox_at < agent_at,
            "sandbox must be a grok top-level flag"
        );
        assert_ne!(
            plan.args.get(agent_at + 1).map(String::as_str),
            Some("--sandbox")
        );
    }

    #[test]
    fn rejects_untrusted_and_on_request() {
        let program = Path::new("grok");
        let cwd = Path::new("/tmp/samchi-launch");
        for approval in [ApprovalPolicy::Untrusted, ApprovalPolicy::OnRequest] {
            let err = plan_launch(&req(
                program,
                cwd,
                approval,
                ThreadSandbox::WorkspaceWrite,
                ExtraSpawnFields::default(),
            ))
            .expect_err("must refuse");
            match err {
                LaunchError::Unenforceable { field, .. } => assert_eq!(field, "approvalPolicy"),
            }
        }
    }

    #[test]
    fn rejects_read_only_and_extra_fields() {
        let program = Path::new("grok");
        let cwd = Path::new("/tmp/samchi-launch");
        let err = plan_launch(&req(
            program,
            cwd,
            ApprovalPolicy::Never,
            ThreadSandbox::ReadOnly,
            ExtraSpawnFields::default(),
        ))
        .expect_err("read-only");
        match err {
            LaunchError::Unenforceable { field, .. } => assert_eq!(field, "sandbox"),
        }

        let extra = ExtraSpawnFields {
            network_access: Some(false),
            ..ExtraSpawnFields::default()
        };
        let err = plan_launch(&req(
            program,
            cwd,
            ApprovalPolicy::Never,
            ThreadSandbox::WorkspaceWrite,
            extra,
        ))
        .expect_err("extra");
        match err {
            LaunchError::Unenforceable { field, .. } => assert_eq!(field, "networkAccess"),
        }
    }

    #[test]
    fn rejects_relative_cwd() {
        let err = plan_launch(&req(
            Path::new("grok"),
            Path::new("relative"),
            ApprovalPolicy::Never,
            ThreadSandbox::WorkspaceWrite,
            ExtraSpawnFields::default(),
        ))
        .expect_err("relative cwd");
        match err {
            LaunchError::Unenforceable { field, .. } => assert_eq!(field, "cwd"),
        }
    }
}
