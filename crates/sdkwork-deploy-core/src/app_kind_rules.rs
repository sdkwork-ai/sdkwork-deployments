//! Application-kind, platform, and package-format compatibility rules and
//! per-format validation ceilings (REQ-2026-0002 requirement 8).

/// WeChat mini-program main package ceiling in bytes (2 MiB).
pub const WECHAT_MINIPROGRAM_MAIN_PACKAGE_BYTES: u64 = 2 * 1024 * 1024;
/// WeChat mini-program total package ceiling in bytes (20 MiB).
pub const WECHAT_MINIPROGRAM_TOTAL_PACKAGE_BYTES: u64 = 20 * 1024 * 1024;
/// Douyin mini-program main package ceiling in bytes (2 MiB).
pub const DOUYIN_MINIPROGRAM_MAIN_PACKAGE_BYTES: u64 = 2 * 1024 * 1024;
/// Douyin mini-program total package ceiling in bytes (20 MiB).
pub const DOUYIN_MINIPROGRAM_TOTAL_PACKAGE_BYTES: u64 = 20 * 1024 * 1024;
/// Generic web bundle ceiling in bytes (256 MiB).
pub const WEB_BUNDLE_MAXIMUM_BYTES: u64 = 256 * 1024 * 1024;
/// Generic process bundle ceiling in bytes (512 MiB).
pub const PROCESS_BUNDLE_MAXIMUM_BYTES: u64 = 512 * 1024 * 1024;
/// Desktop installer ceiling in bytes (2 GiB): Electron/Tauri installers,
/// DMG/PKG, and AppImage payloads exceed mobile-scale ceilings.
pub const DESKTOP_INSTALLER_MAXIMUM_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// JVM artifact ceiling in bytes (512 MiB).
pub const JVM_ARTIFACT_MAXIMUM_BYTES: u64 = 512 * 1024 * 1024;

/// Platform identity field required per platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequiredIdentityField {
    BundleId,
    PackageName,
    AppId,
    BundleName,
    None,
}

impl RequiredIdentityField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BundleId => "bundleId",
            Self::PackageName => "packageName",
            Self::AppId => "appId",
            Self::BundleName => "bundleName",
            Self::None => "none",
        }
    }
}

/// Returns the platform identity field a platform requires before release.
pub fn required_identity_field(platform: &str) -> RequiredIdentityField {
    match platform {
        "IOS" => RequiredIdentityField::BundleId,
        "MACOS" => RequiredIdentityField::BundleId,
        "ANDROID" => RequiredIdentityField::PackageName,
        "WECHAT" | "DOUYIN" => RequiredIdentityField::AppId,
        "WINDOWS" => RequiredIdentityField::AppId,
        "HARMONYOS" => RequiredIdentityField::BundleName,
        _ => RequiredIdentityField::None,
    }
}

/// Validates that an application kind may target the given platform.
pub fn validate_app_kind_platform(app_kind: &str, platform: &str) -> Result<(), String> {
    let allowed: &[&str] = match app_kind {
        "STATIC_WEB" | "SPA_WEB" | "H5_WEB" | "PC_WEB" => &["WEB"],
        "API_SERVICE" => &["API"],
        "WECHAT_MINIPROGRAM" => &["WECHAT"],
        "DOUYIN_MINIPROGRAM" => &["DOUYIN"],
        "IOS_APP" => &["IOS"],
        "ANDROID_APP" => &["ANDROID"],
        "HARMONYOS_APP" => &["HARMONYOS"],
        // A Flutter application is one codebase delivered to several stores, so
        // its *platform targets* name the concrete platforms. The endpoint kind
        // itself therefore admits every platform Flutter ships to.
        "FLUTTER_APP" => &[
            "IOS",
            "ANDROID",
            "HARMONYOS",
            "WINDOWS",
            "MACOS",
            "LINUX",
            "WEB",
        ],
        "DESKTOP_APP" => &["WINDOWS", "MACOS", "LINUX"],
        _ => return Err(format!("unknown app kind {app_kind}")),
    };
    if allowed.contains(&platform) {
        Ok(())
    } else {
        Err(format!(
            "app kind {app_kind} cannot target platform {platform}; allowed: {}",
            allowed.join(", ")
        ))
    }
}

/// Validates that a package format is compatible with the platform target.
pub fn validate_package_format_for_platform(
    platform: &str,
    package_format: &str,
) -> Result<(), String> {
    let allowed: &[&str] = match platform {
        "WEB" => &["DIST_DIR", "TAR_GZ", "ZIP"],
        "API" => &["OCI_IMAGE", "PROCESS_BUNDLE", "TAR_GZ", "JAR", "WAR"],
        "WECHAT" | "DOUYIN" => &["ZIP", "TAR_GZ"],
        "IOS" => &["IPA", "XCARCHIVE", "ZIP"],
        "ANDROID" => &["APK", "AAB", "ZIP"],
        "HARMONYOS" => &["HAP", "APP", "ZIP"],
        "WINDOWS" => &["MSI", "NSIS", "MSIX", "EXE", "ZIP", "TAR_GZ"],
        "MACOS" => &["DMG", "PKG", "ZIP"],
        "LINUX" => &["DEB", "RPM", "APPIMAGE", "TAR_GZ"],
        _ => return Err(format!("unknown platform {platform}")),
    };
    if allowed.contains(&package_format) {
        Ok(())
    } else {
        Err(format!(
            "package format {package_format} is not allowed for platform {platform}; allowed: {}",
            allowed.join(", ")
        ))
    }
}

/// Returns the byte ceiling for a package format; `None` means the platform
/// safety ceiling applies (no additional ceiling).
pub fn package_size_ceiling(platform: &str, package_format: &str) -> Option<u64> {
    match (platform, package_format) {
        ("WECHAT", _) => Some(WECHAT_MINIPROGRAM_TOTAL_PACKAGE_BYTES),
        ("DOUYIN", _) => Some(DOUYIN_MINIPROGRAM_TOTAL_PACKAGE_BYTES),
        ("WEB", "DIST_DIR" | "TAR_GZ") => Some(WEB_BUNDLE_MAXIMUM_BYTES),
        ("API", "PROCESS_BUNDLE" | "JAR" | "WAR") => Some(PROCESS_BUNDLE_MAXIMUM_BYTES),
        ("WINDOWS" | "MACOS" | "LINUX", _) => Some(DESKTOP_INSTALLER_MAXIMUM_BYTES),
        _ => None,
    }
}

/// Validates package size against the per-format ceiling.
pub fn validate_package_size(
    platform: &str,
    package_format: &str,
    size_bytes: u64,
) -> Result<(), String> {
    if size_bytes == 0 {
        return Err("package size must be positive".into());
    }
    if let Some(ceiling) = package_size_ceiling(platform, package_format) {
        if size_bytes > ceiling {
            return Err(format!(
                "package size {size_bytes} exceeds the {ceiling}-byte ceiling for {platform} {package_format}"
            ));
        }
    }
    Ok(())
}

/// Validates a platform identity value for the platform (bounded, no secrets).
pub fn validate_platform_identity(platform: &str, identity: &str) -> Result<(), String> {
    let field = required_identity_field(platform);
    if field == RequiredIdentityField::None {
        return Ok(());
    }
    let field_name = field.as_str();
    if identity.is_empty() || identity.len() > 255 {
        return Err(format!(
            "platform identity {field_name} must be 1..=255 characters"
        ));
    }
    if !identity
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(format!(
            "platform identity {field_name} contains invalid characters"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_kind_platform_matrix_is_enforced() {
        assert!(validate_app_kind_platform("STATIC_WEB", "WEB").is_ok());
        assert!(validate_app_kind_platform("SPA_WEB", "WEB").is_ok());
        assert!(validate_app_kind_platform("H5_WEB", "WEB").is_ok());
        assert!(validate_app_kind_platform("PC_WEB", "WEB").is_ok());
        assert!(validate_app_kind_platform("H5_WEB", "IOS").is_err());
        assert!(validate_app_kind_platform("FLUTTER_APP", "IOS").is_ok());
        assert!(validate_app_kind_platform("FLUTTER_APP", "ANDROID").is_ok());
        assert!(validate_app_kind_platform("FLUTTER_APP", "WEB").is_ok());
        assert!(validate_app_kind_platform("FLUTTER_APP", "WECHAT").is_err());
        assert!(validate_app_kind_platform("API_SERVICE", "API").is_ok());
        assert!(validate_app_kind_platform("WECHAT_MINIPROGRAM", "WECHAT").is_ok());
        assert!(validate_app_kind_platform("DOUYIN_MINIPROGRAM", "DOUYIN").is_ok());
        assert!(validate_app_kind_platform("IOS_APP", "IOS").is_ok());
        assert!(validate_app_kind_platform("ANDROID_APP", "ANDROID").is_ok());
        assert!(validate_app_kind_platform("HARMONYOS_APP", "HARMONYOS").is_ok());
        assert!(validate_app_kind_platform("IOS_APP", "ANDROID").is_err());
        assert!(validate_app_kind_platform("WECHAT_MINIPROGRAM", "IOS").is_err());
        assert!(validate_app_kind_platform("UNKNOWN", "WEB").is_err());
    }

    #[test]
    fn package_format_matrix_is_enforced() {
        assert!(validate_package_format_for_platform("IOS", "IPA").is_ok());
        assert!(validate_package_format_for_platform("IOS", "APK").is_err());
        assert!(validate_package_format_for_platform("ANDROID", "APK").is_ok());
        assert!(validate_package_format_for_platform("ANDROID", "AAB").is_ok());
        assert!(validate_package_format_for_platform("ANDROID", "IPA").is_err());
        assert!(validate_package_format_for_platform("HARMONYOS", "HAP").is_ok());
        assert!(validate_package_format_for_platform("HARMONYOS", "APP").is_ok());
        assert!(validate_package_format_for_platform("API", "OCI_IMAGE").is_ok());
        assert!(validate_package_format_for_platform("API", "APK").is_err());
        assert!(validate_package_format_for_platform("WEB", "DIST_DIR").is_ok());
    }

    #[test]
    fn mini_program_size_ceilings_apply() {
        assert!(
            validate_package_size("WECHAT", "ZIP", WECHAT_MINIPROGRAM_TOTAL_PACKAGE_BYTES).is_ok()
        );
        assert!(
            validate_package_size("WECHAT", "ZIP", WECHAT_MINIPROGRAM_TOTAL_PACKAGE_BYTES + 1)
                .is_err()
        );
        assert!(
            validate_package_size("DOUYIN", "ZIP", DOUYIN_MINIPROGRAM_TOTAL_PACKAGE_BYTES).is_ok()
        );
        assert!(validate_package_size("ANDROID", "APK", 10 * 1024 * 1024 * 1024).is_ok());
        assert!(validate_package_size("ANDROID", "APK", 0).is_err());
    }

    #[test]
    fn platform_identity_is_validated() {
        assert!(validate_platform_identity("IOS", "com.sdkwork.example").is_ok());
        assert!(validate_platform_identity("ANDROID", "com.sdkwork.example").is_ok());
        assert!(validate_platform_identity("WECHAT", "wx0123456789abcdef").is_ok());
        assert!(validate_platform_identity("IOS", "com.sdkwork.example/api").is_err());
        assert!(validate_platform_identity("IOS", "").is_err());
        assert!(validate_platform_identity("WEB", "").is_ok());
    }

    #[test]
    fn desktop_platform_matrix_is_enforced() {
        assert!(validate_app_kind_platform("DESKTOP_APP", "WINDOWS").is_ok());
        assert!(validate_app_kind_platform("DESKTOP_APP", "MACOS").is_ok());
        assert!(validate_app_kind_platform("DESKTOP_APP", "LINUX").is_ok());
        assert!(validate_app_kind_platform("DESKTOP_APP", "ANDROID").is_err());
        assert!(validate_app_kind_platform("IOS_APP", "MACOS").is_err());

        assert!(validate_package_format_for_platform("WINDOWS", "MSI").is_ok());
        assert!(validate_package_format_for_platform("WINDOWS", "NSIS").is_ok());
        assert!(validate_package_format_for_platform("WINDOWS", "MSIX").is_ok());
        assert!(validate_package_format_for_platform("WINDOWS", "EXE").is_ok());
        assert!(validate_package_format_for_platform("WINDOWS", "DMG").is_err());
        assert!(validate_package_format_for_platform("MACOS", "DMG").is_ok());
        assert!(validate_package_format_for_platform("MACOS", "PKG").is_ok());
        assert!(validate_package_format_for_platform("MACOS", "MSI").is_err());
        assert!(validate_package_format_for_platform("LINUX", "DEB").is_ok());
        assert!(validate_package_format_for_platform("LINUX", "RPM").is_ok());
        assert!(validate_package_format_for_platform("LINUX", "APPIMAGE").is_ok());
        assert!(validate_package_format_for_platform("LINUX", "EXE").is_err());
        assert!(validate_package_format_for_platform("API", "JAR").is_ok());
        assert!(validate_package_format_for_platform("API", "WAR").is_ok());
    }

    #[test]
    fn desktop_identity_and_ceiling_rules_apply() {
        assert!(validate_platform_identity("MACOS", "com.sdkwork.example").is_ok());
        assert!(validate_platform_identity("WINDOWS", "SdkWork.Desktop").is_ok());
        assert!(validate_platform_identity("LINUX", "").is_ok());

        assert!(validate_package_size("WINDOWS", "NSIS", DESKTOP_INSTALLER_MAXIMUM_BYTES).is_ok());
        assert!(
            validate_package_size("MACOS", "DMG", DESKTOP_INSTALLER_MAXIMUM_BYTES + 1).is_err()
        );
        assert!(
            validate_package_size("LINUX", "APPIMAGE", DESKTOP_INSTALLER_MAXIMUM_BYTES).is_ok()
        );
        assert!(validate_package_size("API", "JAR", JVM_ARTIFACT_MAXIMUM_BYTES).is_ok());
        assert!(validate_package_size("API", "WAR", JVM_ARTIFACT_MAXIMUM_BYTES + 1).is_err());
    }
}
