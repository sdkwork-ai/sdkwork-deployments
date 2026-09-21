use chrono::{SecondsFormat, Utc};
use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_deploy_contract::DeployServiceError;
use sqlx::postgres::PgRow;
use sqlx::{Error as SqlxError, PgPool, Row};

pub(crate) fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(crate) fn is_unique_violation(error: &SqlxError) -> bool {
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|code| code == "23505")
        .unwrap_or(false)
}

pub(crate) fn store_error(context: &str, error: SqlxError) -> DeployServiceError {
    tracing::error!("{context}: {error}");
    match error {
        SqlxError::Database(db) if db.is_unique_violation() => {
            DeployServiceError::conflict(db.message())
        }
        SqlxError::RowNotFound => DeployServiceError::not_found("resource not found"),
        _ => DeployServiceError::Internal(format!("{context}: {error}")),
    }
}

pub(crate) fn pagination(page: i32, page_size: i32) -> (i32, i32, i64) {
    let (page, page_size) = sdkwork_deploy_core::normalize_pagination(page, page_size);
    let offset = sdkwork_deploy_core::pagination_offset(page, page_size);
    (page, page_size, offset)
}

pub(crate) fn next_id(generator: &SnowflakeIdGenerator) -> Result<i64, DeployServiceError> {
    generator
        .generate()
        .map_err(|error| DeployServiceError::Internal(error.to_string()))
}

pub(crate) fn new_uuid() -> String {
    sdkwork_database_id::uuid_v4()
}

pub(crate) fn sha256_hex(content: &str) -> String {
    sdkwork_utils_rust::sha256_hash(content.as_bytes())
}

pub(crate) fn bool_from_row(row: &PgRow, column: &str) -> Result<bool, SqlxError> {
    row.try_get(column)
}

pub(crate) fn json_from_row(
    row: &PgRow,
    column: &str,
) -> Result<Option<serde_json::Value>, SqlxError> {
    row.try_get(column)
}

/// Read a string list out of a `JSONB` column.
///
/// The read-side counterpart of [`string_list_to_json`], and it exists for the
/// same reason: sqlx maps `Vec<String>` to `TEXT[]`, so
/// `try_get::<Option<Vec<String>>, _>` on a `JSONB` column fails with
///
/// ```text
/// mismatched types; Rust type `Option<Vec<String>>` (as SQL type `TEXT[]`)
/// is not compatible with SQL type `JSONB`
/// ```
///
/// Decoding through `serde_json::Value` is the only correct route. Doing it here
/// also keeps callers from writing `row.try_get("col").ok().flatten()`, which
/// turns a real decode failure into a silent `None` — the override then looks
/// unset and the platform catalog is used instead, with nothing in the logs.
///
/// A column that is `NULL` decodes to `None`; a value that is not a JSON array of
/// strings is a decode error and is surfaced rather than swallowed.
pub(crate) fn string_list_from_row(
    row: &PgRow,
    column: &str,
) -> Result<Option<Vec<String>>, SqlxError> {
    let Some(value) = json_from_row(row, column)? else {
        return Ok(None);
    };
    let malformed = || {
        SqlxError::Decode(
            format!("column `{column}` must hold a JSON array of strings, got {value}").into(),
        )
    };
    let array = value.as_array().ok_or_else(malformed)?;
    // A `null` entry is a corruption signal, not a value to skip: the writer
    // only ever emits strings.
    array
        .iter()
        .map(|entry| entry.as_str().map(str::to_owned).ok_or_else(malformed))
        .collect::<Result<Vec<String>, SqlxError>>()
        .map(Some)
}

/// Encode a string list for a `JSONB` column.
///
/// The write-side counterpart of [`string_list_from_row`], and the reason it
/// exists: binding a bare `Vec<String>` makes sqlx infer `TEXT[]` from the Rust
/// type, so a `JSONB` target is rejected at execution time with
///
/// ```text
/// column "<col>" is of type jsonb but expression is of type text[]
/// ```
///
/// That failure only surfaces when the statement actually runs, so it hides
/// behind `Option::None` in unit tests and reaches production as a masked 500.
/// Routing every `Vec<String>` → JSONB write through this helper keeps the
/// `JSONB` contract in one place instead of restating `Value::Array(..)` at each
/// call site, where it is easy to forget.
///
/// `None` stays `None` — the caller decides whether that means "leave the column
/// alone" or "fall back to the column DEFAULT".
pub(crate) fn string_list_to_json(value: Option<&Vec<String>>) -> Option<serde_json::Value> {
    value.map(|items| {
        serde_json::Value::Array(
            items
                .iter()
                .cloned()
                .map(serde_json::Value::String)
                .collect(),
        )
    })
}

pub(crate) fn datetime_from_row(row: &PgRow, column: &str) -> Result<String, SqlxError> {
    row.try_get::<chrono::DateTime<Utc>, _>(column)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
}

pub(crate) fn required_datetime(row: &PgRow, column: &str) -> Result<String, DeployServiceError> {
    datetime_from_row(row, column)
        .map_err(|error| DeployServiceError::Internal(format!("read {column}: {error}")))
}

pub(crate) fn optional_datetime(
    row: &PgRow,
    column: &str,
) -> Result<Option<String>, DeployServiceError> {
    optional_datetime_from_row(row, column)
        .map_err(|error| DeployServiceError::Internal(format!("read {column}: {error}")))
}

pub(crate) fn optional_datetime_from_row(
    row: &PgRow,
    column: &str,
) -> Result<Option<String>, SqlxError> {
    row.try_get::<Option<chrono::DateTime<Utc>>, _>(column)
        .map(|value| value.map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true)))
}

pub(crate) async fn resolve_app_uuid(
    pool: &PgPool,
    tenant_id: i64,
    app_internal_id: i64,
) -> Result<String, DeployServiceError> {
    let row = sqlx::query(
        "SELECT uuid FROM deploy_app
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(app_internal_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("resolve deploy_app uuid", error))?;

    row.and_then(|row| row.try_get::<String, _>("uuid").ok())
        .ok_or_else(|| DeployServiceError::not_found("app not found"))
}

pub(crate) async fn resolve_node_cluster_internal_id(
    pool: &PgPool,
    tenant_id: i64,
    cluster_uuid: &str,
) -> Result<i64, DeployServiceError> {
    let row = sqlx::query(
        "SELECT id FROM deploy_node_cluster
         WHERE tenant_id = $1 AND uuid = $2",
    )
    .bind(tenant_id)
    .bind(cluster_uuid)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("resolve deploy_node_cluster id", error))?;

    row.and_then(|row| row.try_get::<i64, _>("id").ok())
        .ok_or_else(|| DeployServiceError::not_found("cluster not found"))
}

pub(crate) async fn resolve_app_internal_id(
    pool: &PgPool,
    tenant_id: i64,
    app_uuid: &str,
) -> Result<i64, DeployServiceError> {
    let row = sqlx::query(
        "SELECT id FROM deploy_app
         WHERE tenant_id = $1 AND uuid = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(app_uuid)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("resolve deploy_app id", error))?;

    row.and_then(|row| row.try_get::<i64, _>("id").ok())
        .ok_or_else(|| DeployServiceError::not_found("app not found"))
}

pub(crate) async fn resolve_platform_target_internal_id(
    pool: &PgPool,
    tenant_id: i64,
    app_internal_id: i64,
    target_uuid: &str,
) -> Result<i64, DeployServiceError> {
    let row = sqlx::query(
        "SELECT id FROM deploy_app_platform_target
         WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(app_internal_id)
    .bind(target_uuid)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("resolve deploy_app_platform_target id", error))?;

    row.and_then(|row| row.try_get::<i64, _>("id").ok())
        .ok_or_else(|| DeployServiceError::not_found("platform target not found"))
}

pub(crate) async fn resolve_build_internal_id(
    pool: &PgPool,
    tenant_id: i64,
    app_internal_id: i64,
    build_uuid: &str,
) -> Result<i64, DeployServiceError> {
    let row = sqlx::query(
        "SELECT id FROM deploy_build
         WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(app_internal_id)
    .bind(build_uuid)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("resolve deploy_build id", error))?;

    row.and_then(|row| row.try_get::<i64, _>("id").ok())
        .ok_or_else(|| DeployServiceError::not_found("build not found"))
}

pub(crate) async fn resolve_package_internal_id(
    pool: &PgPool,
    tenant_id: i64,
    app_internal_id: i64,
    package_uuid: &str,
) -> Result<i64, DeployServiceError> {
    let row = sqlx::query(
        "SELECT id FROM deploy_package
         WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(app_internal_id)
    .bind(package_uuid)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("resolve deploy_package id", error))?;

    row.and_then(|row| row.try_get::<i64, _>("id").ok())
        .ok_or_else(|| DeployServiceError::not_found("package not found"))
}

pub(crate) async fn resolve_app_release_internal_id(
    pool: &PgPool,
    tenant_id: i64,
    app_internal_id: i64,
    release_uuid: &str,
) -> Result<i64, DeployServiceError> {
    let row = sqlx::query(
        "SELECT id FROM deploy_release
         WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND release_status IS NOT NULL",
    )
    .bind(tenant_id)
    .bind(app_internal_id)
    .bind(release_uuid)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("resolve deploy_release app id", error))?;

    row.and_then(|row| row.try_get::<i64, _>("id").ok())
        .ok_or_else(|| DeployServiceError::not_found("release not found"))
}

pub(crate) async fn resolve_channel_internal_id(
    pool: &PgPool,
    tenant_id: i64,
    app_internal_id: i64,
    channel_uuid: &str,
) -> Result<i64, DeployServiceError> {
    let row = sqlx::query(
        "SELECT id FROM deploy_release_channel
         WHERE tenant_id = $1 AND app_id = $2 AND uuid = $3 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(app_internal_id)
    .bind(channel_uuid)
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("resolve deploy_release_channel id", error))?;

    row.and_then(|row| row.try_get::<i64, _>("id").ok())
        .ok_or_else(|| DeployServiceError::not_found("channel not found"))
}

/// Encodes an opaque keyset cursor for `(sort_instant, id)` ordered lists
/// (PAGINATION_SPEC §6). The payload is base64 of `v1|<created_at>|<id>`;
/// clients must never parse it.
pub(crate) fn encode_keyset_cursor(created_at: &str, id: i64) -> String {
    use base64::Engine as _;
    let payload = format!("v1|{created_at}|{id}");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.as_bytes())
}

/// Decodes a keyset cursor produced by [`encode_keyset_cursor`]; returns
/// `None` for malformed, oversized, or out-of-range tokens so callers fail
/// closed.
pub(crate) fn decode_keyset_cursor(token: &str) -> Option<(String, i64)> {
    use base64::Engine as _;
    if token.is_empty() || token.len() > 512 {
        return None;
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(token.as_bytes())
        .ok()?;
    let payload = std::str::from_utf8(&decoded).ok()?;
    let mut parts = payload.splitn(3, '|');
    let version = parts.next()?;
    let created_at = parts.next()?;
    let id = parts.next()?;
    if version != "v1" || created_at.is_empty() || created_at.len() > 64 {
        return None;
    }
    let id = id.parse::<i64>().ok()?;
    if id <= 0 {
        return None;
    }
    if chrono::DateTime::parse_from_rfc3339(created_at).is_err() {
        return None;
    }
    Some((created_at.to_string(), id))
}

#[cfg(test)]
mod json_write_tests {
    use super::string_list_to_json;

    /// A JSONB write must produce a JSON *array*, never a `TEXT[]`-shaped value.
    /// This is the invariant the sqlx binding depends on, and the one whose
    /// absence produced `column "app_domain_suffixes" is of type jsonb but
    /// expression is of type text[]`.
    #[test]
    fn string_list_encodes_as_a_json_array() {
        let suffixes = vec!["sdkwork.com".to_owned(), "sdkwork.dev".to_owned()];
        let encoded = string_list_to_json(Some(&suffixes)).expect("Some encodes to Some");
        assert!(
            encoded.is_array(),
            "a JSONB column needs a JSON array, got {encoded}"
        );
        assert_eq!(
            encoded,
            serde_json::json!(["sdkwork.com", "sdkwork.dev"]),
            "order is preserved verbatim"
        );
    }

    /// `None` must stay `None`: the caller uses it to mean "leave the column
    /// alone" (update) or "fall back to the column DEFAULT" (insert).
    #[test]
    fn absent_list_stays_none_rather_than_encoding_null() {
        assert_eq!(string_list_to_json(None), None);
    }

    /// An empty list is a real value (an empty JSON array), not `None` — the
    /// callers filter emptiness out before this point, so encoding must not
    /// silently collapse it.
    #[test]
    fn empty_list_encodes_as_an_empty_array() {
        let empty = Vec::new();
        let encoded = string_list_to_json(Some(&empty)).expect("Some encodes to Some");
        assert_eq!(encoded, serde_json::json!([]));
    }

    /// The write helper's output must be exactly what the read helper accepts —
    /// that round trip is the contract both call sites depend on.
    #[test]
    fn encoded_json_decodes_back_to_the_same_list() {
        let suffixes = vec!["sdkwork.com".to_owned(), "sdkwork.cn".to_owned()];
        let encoded = string_list_to_json(Some(&suffixes)).expect("encodes");
        // `string_list_from_row` reads `serde_json::Value` and requires a JSON
        // array of strings; assert the shape it will see.
        let array = encoded.as_array().expect("a JSON array");
        let decoded = array
            .iter()
            .map(|entry| entry.as_str().expect("every entry is a string").to_owned())
            .collect::<Vec<_>>();
        assert_eq!(decoded, suffixes);
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_keyset_cursor, encode_keyset_cursor};

    #[test]
    fn keyset_cursor_round_trips() {
        let token = encode_keyset_cursor("2026-08-03T10:00:00.123Z", 42);
        let decoded = decode_keyset_cursor(&token).expect("valid cursor decodes");
        assert_eq!(decoded, ("2026-08-03T10:00:00.123Z".to_string(), 42));
    }

    #[test]
    fn keyset_cursor_is_opaque_and_fails_closed() {
        assert!(decode_keyset_cursor("").is_none());
        assert!(decode_keyset_cursor("not-base64!").is_none());
        assert!(decode_keyset_cursor("v1|2026-08-03T10:00:00Z|not-a-number").is_none());
        assert!(decode_keyset_cursor("v1|2026-08-03T10:00:00Z|0").is_none());
        assert!(decode_keyset_cursor("v1|2026-08-03T10:00:00Z|-5").is_none());
        assert!(decode_keyset_cursor("v1|not-a-date|1").is_none());
        assert!(decode_keyset_cursor("v2|2026-08-03T10:00:00Z|1").is_none());
        assert!(decode_keyset_cursor(&"x".repeat(600)).is_none());
        // 编码结果不得包含客户端可解析的明文分隔结构（opaque）。
        let token = encode_keyset_cursor("2026-08-03T10:00:00Z", 7);
        assert!(!token.contains('|'));
        assert!(!token.contains("2026-08-03"));
    }
}
