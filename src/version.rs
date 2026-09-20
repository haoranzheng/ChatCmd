use std::path::{Path, PathBuf};

const VERSION_MARKER_FILE: &str = "chatcmd-version.txt";

fn embedded_build_version() -> Option<&'static str> {
    option_env!("CHATCMD_BUILD_VERSION").filter(|value| is_valid_version(value))
}

pub(crate) fn compiled_version() -> &'static str {
    embedded_build_version().unwrap_or(env!("CARGO_PKG_VERSION"))
}

pub(crate) fn app_version() -> String {
    select_app_version(
        embedded_build_version(),
        installed_version(),
        env!("CARGO_PKG_VERSION"),
    )
}

fn select_app_version(
    build_version: Option<&str>,
    installed_version: Option<String>,
    package_version: &str,
) -> String {
    if let Some(version) = build_version.filter(|value| is_valid_version(value)) {
        return version.to_owned();
    }
    installed_version.unwrap_or_else(|| package_version.to_owned())
}

pub(crate) fn version_marker_path() -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    let root = install_root(&current_exe)?;
    Some(root.join(VERSION_MARKER_FILE))
}

fn installed_version() -> Option<String> {
    let marker = version_marker_path()?;
    let value = std::fs::read_to_string(marker).ok()?;
    let value = value.trim().trim_start_matches('\u{feff}');
    is_valid_version(value).then(|| value.to_owned())
}

pub(crate) fn install_root(current_exe: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    if let Some(app_bundle) = current_exe.ancestors().find(|path| {
        path.extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("app"))
    }) {
        return app_bundle.parent().map(Path::to_path_buf);
    }

    current_exe.parent().map(Path::to_path_buf)
}

pub(crate) fn is_valid_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | '+')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_supported_version_strings() {
        assert!(is_valid_version("1.4.0"));
        assert!(is_valid_version("26.08.31.1055"));
        assert!(is_valid_version("1.4.0-beta+3"));
        assert!(!is_valid_version(""));
        assert!(!is_valid_version("1.4.0; rm -rf /"));
    }

    #[test]
    fn embedded_build_version_wins_over_stale_install_marker() {
        assert_eq!(
            select_app_version(
                Some("26.09.20.1200"),
                Some("26.09.07.0938".to_owned()),
                "0.1.0",
            ),
            "26.09.20.1200"
        );
    }

    #[test]
    fn install_marker_is_used_when_build_metadata_is_absent() {
        assert_eq!(
            select_app_version(None, Some("26.09.07.0938".to_owned()), "0.1.0"),
            "26.09.07.0938"
        );
    }

    #[test]
    fn package_version_is_the_final_fallback() {
        assert_eq!(select_app_version(None, None, "0.1.0"), "0.1.0");
    }
}
