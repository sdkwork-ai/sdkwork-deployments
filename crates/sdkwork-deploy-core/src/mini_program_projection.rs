//! Project a Deploy build artifact into a WeChat-buildable mini-program tree.
//!
//! Deploy packages carry their own manifest (`sdkwork.deploy-package.v1.json`)
//! so the platform can verify what it built, but WeChat rejects unknown files at
//! the project root and requires a `project.config.json`. Uploading the package
//! as-is therefore fails before review. Projection is the deterministic step
//! between "package validated" and "code uploaded":
//!
//! - drop the Deploy manifest and any stale `project.config.json`,
//! - emit a `project.config.json` bound to the target AppID,
//! - compute the main-package and total size from `app.json` subpackage roots,
//!   and refuse the upload when WeChat's ceilings would reject it anyway.
//!
//! The plan is pure: it reports what to keep and what to generate, and the
//! uploader writes the files. That keeps the size and layout rules testable
//! without a provider or a filesystem.
use serde_json::Value;

use crate::app_kind_rules::{
    WECHAT_MINIPROGRAM_MAIN_PACKAGE_BYTES, WECHAT_MINIPROGRAM_TOTAL_PACKAGE_BYTES,
};

/// The Deploy package manifest file name.
pub const DEPLOY_PACKAGE_MANIFEST: &str = "sdkwork.deploy-package.v1.json";
/// The WeChat project configuration file name.
pub const PROJECT_CONFIG_FILE: &str = "project.config.json";
/// The WeChat application manifest file name.
pub const MINI_PROGRAM_APP_JSON: &str = "app.json";

/// One input entry with its size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionEntry {
    /// Relative path inside the package.
    pub path: String,
    /// Uncompressed size in bytes.
    pub size: u64,
}

/// What the uploader must do with one entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedEntry {
    /// Relative path.
    pub path: String,
    /// Uncompressed size.
    pub size: u64,
    /// True when the entry belongs in the uploaded tree.
    pub keep: bool,
}

/// The projection result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiniProgramProjectionPlan {
    /// Every input entry with its keep/drop decision.
    pub entries: Vec<ProjectedEntry>,
    /// Paths dropped from the uploaded tree.
    pub removed_paths: Vec<String>,
    /// Generated `project.config.json` content.
    pub project_config_json: String,
    /// Bytes in the main package.
    pub main_package_bytes: u64,
    /// Bytes across all kept entries.
    pub total_package_bytes: u64,
    /// Subpackage roots declared by `app.json`.
    pub subpackage_roots: Vec<String>,
}

/// A projection failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiniProgramProjectionError {
    /// `app.json` is missing from the package.
    MissingAppJson,
    /// `app.json` is not valid JSON or has the wrong shape.
    InvalidAppJson(String),
    /// The main package exceeds WeChat's ceiling.
    MainPackageTooLarge {
        /// Measured main-package bytes.
        bytes: u64,
        /// WeChat's ceiling.
        maximum: u64,
    },
    /// The whole package exceeds WeChat's ceiling.
    TotalPackageTooLarge {
        /// Measured total bytes.
        bytes: u64,
        /// WeChat's ceiling.
        maximum: u64,
    },
}

impl std::fmt::Display for MiniProgramProjectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingAppJson => {
                write!(f, "mini program package has no {MINI_PROGRAM_APP_JSON}")
            }
            Self::InvalidAppJson(detail) => write!(f, "app.json is invalid: {detail}"),
            Self::MainPackageTooLarge { bytes, maximum } => write!(
                f,
                "mini program main package is {bytes} bytes, above the {maximum} byte ceiling"
            ),
            Self::TotalPackageTooLarge { bytes, maximum } => write!(
                f,
                "mini program package is {bytes} bytes, above the {maximum} byte ceiling"
            ),
        }
    }
}

impl std::error::Error for MiniProgramProjectionError {}

/// Plans the WeChat project tree with the documented size ceilings.
pub fn plan_mini_program_projection(
    app_id: &str,
    app_json: &str,
    entries: &[ProjectionEntry],
) -> Result<MiniProgramProjectionPlan, MiniProgramProjectionError> {
    plan_mini_program_projection_with_ceilings(
        app_id,
        app_json,
        entries,
        WECHAT_MINIPROGRAM_MAIN_PACKAGE_BYTES,
        WECHAT_MINIPROGRAM_TOTAL_PACKAGE_BYTES,
    )
}

/// Plans the WeChat project tree with explicit ceilings (tests and future
/// provider limits).
pub fn plan_mini_program_projection_with_ceilings(
    app_id: &str,
    app_json: &str,
    entries: &[ProjectionEntry],
    main_maximum_bytes: u64,
    total_maximum_bytes: u64,
) -> Result<MiniProgramProjectionPlan, MiniProgramProjectionError> {
    if !entries
        .iter()
        .any(|entry| normalize(&entry.path) == MINI_PROGRAM_APP_JSON)
    {
        return Err(MiniProgramProjectionError::MissingAppJson);
    }
    let subpackage_roots = parse_subpackage_roots(app_json)?;
    let mut projected = Vec::with_capacity(entries.len());
    let mut removed_paths = Vec::new();
    let mut main_package_bytes = 0u64;
    let mut total_package_bytes = 0u64;
    for entry in entries {
        let path = normalize(&entry.path);
        let keep = path != DEPLOY_PACKAGE_MANIFEST && path != PROJECT_CONFIG_FILE;
        if !keep {
            removed_paths.push(path.clone());
        } else {
            total_package_bytes = total_package_bytes.saturating_add(entry.size);
            if !in_subpackage(&path, &subpackage_roots) {
                main_package_bytes = main_package_bytes.saturating_add(entry.size);
            }
        }
        projected.push(ProjectedEntry {
            path,
            size: entry.size,
            keep,
        });
    }
    if main_package_bytes > main_maximum_bytes {
        return Err(MiniProgramProjectionError::MainPackageTooLarge {
            bytes: main_package_bytes,
            maximum: main_maximum_bytes,
        });
    }
    if total_package_bytes > total_maximum_bytes {
        return Err(MiniProgramProjectionError::TotalPackageTooLarge {
            bytes: total_package_bytes,
            maximum: total_maximum_bytes,
        });
    }
    Ok(MiniProgramProjectionPlan {
        entries: projected,
        removed_paths,
        project_config_json: project_config_json(app_id),
        main_package_bytes,
        total_package_bytes,
        subpackage_roots,
    })
}

/// The generated `project.config.json` for one AppID.
pub fn project_config_json(app_id: &str) -> String {
    serde_json::json!({
        "appid": app_id.trim(),
        "compileType": "miniprogram",
        "miniprogramRoot": "./",
        "projectname": "sdkwork-cloudrouter",
        "setting": {
            "es6": true,
            "minified": true,
            "postcss": false,
        },
    })
    .to_string()
}

fn parse_subpackage_roots(app_json: &str) -> Result<Vec<String>, MiniProgramProjectionError> {
    let value: Value = serde_json::from_str(app_json)
        .map_err(|error| MiniProgramProjectionError::InvalidAppJson(error.to_string()))?;
    let Some(object) = value.as_object() else {
        return Err(MiniProgramProjectionError::InvalidAppJson(
            "app.json must be a JSON object".to_string(),
        ));
    };
    let mut roots = Vec::new();
    for key in ["subPackages", "subpackages"] {
        let Some(subpackages) = object.get(key) else {
            continue;
        };
        let Some(subpackages) = subpackages.as_array() else {
            return Err(MiniProgramProjectionError::InvalidAppJson(format!(
                "{key} must be an array"
            )));
        };
        for subpackage in subpackages {
            let Some(root) = subpackage.get("root").and_then(Value::as_str) else {
                return Err(MiniProgramProjectionError::InvalidAppJson(format!(
                    "{key} entry is missing root"
                )));
            };
            roots.push(root.trim_matches('/').to_string());
        }
    }
    Ok(roots)
}

fn in_subpackage(path: &str, roots: &[String]) -> bool {
    roots
        .iter()
        .any(|root| !root.is_empty() && (path == root || path.starts_with(&format!("{root}/"))))
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, size: u64) -> ProjectionEntry {
        ProjectionEntry {
            path: path.to_string(),
            size,
        }
    }

    const APP_JSON: &str = r#"{
        "pages": ["pages/index/index"],
        "subPackages": [{ "root": "packageA", "pages": ["pages/a"] }]
    }"#;

    #[test]
    fn drops_the_deploy_manifest_and_regenerates_project_config() {
        let plan = plan_mini_program_projection(
            "wxappid",
            APP_JSON,
            &[
                entry("app.json", 500),
                entry("pages/index/index.js", 1_000),
                entry("sdkwork.deploy-package.v1.json", 300),
                entry("project.config.json", 200),
                entry("packageA/pages/a.js", 700),
            ],
        )
        .expect("plan");
        assert_eq!(
            plan.removed_paths,
            vec![
                "sdkwork.deploy-package.v1.json".to_string(),
                "project.config.json".to_string()
            ]
        );
        assert_eq!(plan.main_package_bytes, 1_500);
        assert_eq!(plan.total_package_bytes, 2_200);
        assert_eq!(plan.subpackage_roots, vec!["packageA".to_string()]);
        let config: Value = serde_json::from_str(&plan.project_config_json).expect("config json");
        assert_eq!(config["appid"], "wxappid");
        assert_eq!(config["compileType"], "miniprogram");
    }

    #[test]
    fn requires_app_json() {
        let error = plan_mini_program_projection("wxappid", APP_JSON, &[entry("index.js", 1)])
            .expect_err("missing app.json");
        assert_eq!(error, MiniProgramProjectionError::MissingAppJson);
    }

    #[test]
    fn rejects_invalid_app_json() {
        let error = plan_mini_program_projection("wxappid", "not-json", &[entry("app.json", 1)])
            .expect_err("invalid app.json");
        assert!(matches!(
            error,
            MiniProgramProjectionError::InvalidAppJson(_)
        ));
    }

    #[test]
    fn enforces_both_size_ceilings() {
        let entries = [entry("app.json", 10), entry("packageA/a.js", 10)];
        let error =
            plan_mini_program_projection_with_ceilings("wxappid", APP_JSON, &entries, 5, 1_000)
                .expect_err("main package too large");
        assert!(matches!(
            error,
            MiniProgramProjectionError::MainPackageTooLarge { .. }
        ));
        let error =
            plan_mini_program_projection_with_ceilings("wxappid", APP_JSON, &entries, 1_000, 15)
                .expect_err("total too large");
        assert!(matches!(
            error,
            MiniProgramProjectionError::TotalPackageTooLarge { .. }
        ));
    }

    #[test]
    fn normalizes_windows_paths_and_supports_lowercase_subpackages() {
        let app_json = r#"{ "subpackages": [{ "root": "/packageB/" }] }"#;
        let plan = plan_mini_program_projection(
            "wxappid",
            app_json,
            &[
                ProjectionEntry {
                    path: "app.json".to_string(),
                    size: 1,
                },
                ProjectionEntry {
                    path: "packageB\\page.js".to_string(),
                    size: 5,
                },
            ],
        )
        .expect("plan");
        assert_eq!(plan.subpackage_roots, vec!["packageB".to_string()]);
        assert_eq!(plan.main_package_bytes, 1);
    }
}
