//! Query-parameter parsing that `serde_urlencoded` cannot express.
//!
//! `axum::extract::Query<T>` deserializes a struct through
//! `serde_urlencoded`, whose struct visitor rejects a repeated key
//! (`app_kind=A&app_kind=B` becomes a "duplicate field" error). The published
//! contract nonetheless declares `app_kind` as `style: form, explode: true`,
//! because that is what generated SDKs know how to send, so the value has to be
//! read from the raw query instead of from a struct field.

use sdkwork_deploy_contract::{AppKind, DeployServiceError};

/// The `app_kind` values named by a raw query string.
///
/// Both shapes a client can produce are accepted: a repeated key (what the
/// generated SDKs send for an exploded array parameter) and one
/// comma-separated value (what a hand-written URL or a cleared multi-select
/// typically sends). Blank entries are ignored; an unknown kind is a validation
/// error, because a filter that silently matches nothing looks identical to a
/// tenant with no apps of that kind.
pub fn app_kind_filter(raw_query: Option<&str>) -> Result<Vec<AppKind>, DeployServiceError> {
    let Some(raw_query) = raw_query else {
        return Ok(Vec::new());
    };
    let values: Vec<String> = url::form_urlencoded::parse(raw_query.as_bytes())
        .filter(|(key, _)| key == "app_kind")
        .map(|(_, value)| value.into_owned())
        .collect();
    AppKind::parse_filter(values.iter().map(String::as_str)).map_err(DeployServiceError::validation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_values_and_comma_lists_parse_alike() {
        let repeated = app_kind_filter(Some("app_kind=H5_WEB&app_kind=PC_WEB")).unwrap();
        let comma = app_kind_filter(Some("app_kind=H5_WEB%2CPC_WEB")).unwrap();
        assert_eq!(repeated, comma);
        assert_eq!(repeated, vec![AppKind::H5Web, AppKind::PcWeb]);
    }

    #[test]
    fn other_query_parameters_are_ignored() {
        let parsed =
            app_kind_filter(Some("page=2&app_kind=FLUTTER_APP&ownership_scope=TENANT")).unwrap();
        assert_eq!(parsed, vec![AppKind::FlutterApp]);
    }

    #[test]
    fn absent_blank_and_empty_lists_mean_every_kind() {
        assert!(app_kind_filter(None).unwrap().is_empty());
        assert!(app_kind_filter(Some("app_kind=")).unwrap().is_empty());
        assert!(app_kind_filter(Some("app_kind=%2C")).unwrap().is_empty());
    }

    #[test]
    fn unknown_and_duplicate_values_are_handled() {
        assert!(app_kind_filter(Some("app_kind=NOT_A_KIND")).is_err());
        let duplicated = app_kind_filter(Some("app_kind=IOS_APP&app_kind=IOS_APP")).unwrap();
        assert_eq!(duplicated, vec![AppKind::IosApp]);
    }
}
