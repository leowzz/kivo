//! Validation is independent of installed apps so backups remain portable.

pub(crate) fn valid_application_path(value: &str) -> bool {
    if value.is_empty() || value.len() > 2_048 || value.chars().any(char::is_control) {
        return false;
    }
    let bytes = value.as_bytes();
    let absolute = value.starts_with('/')
        || value.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'/' | b'\\'));
    let lower = value.to_ascii_lowercase();
    absolute
        && [".app", ".exe", ".com", ".lnk"]
            .iter()
            .any(|ext| lower.ends_with(ext))
}

pub(crate) fn valid_website_url(value: &str) -> bool {
    if value.len() > 2_048
        || value
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return false;
    }
    tauri::Url::parse(value).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

#[cfg(not(feature = "product-studio"))]
pub(crate) fn installed_application(value: &str) -> bool {
    let path = std::path::Path::new(value);
    if !valid_application_path(value) || !path.is_absolute() {
        return false;
    }
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    #[cfg(target_os = "macos")]
    return extension.eq_ignore_ascii_case("app") && path.is_dir();
    #[cfg(target_os = "windows")]
    return ["exe", "com", "lnk"]
        .iter()
        .any(|ext| extension.eq_ignore_ascii_case(ext))
        && path.is_file();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = extension;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_targets_reject_invalid_schemes_credentials_and_controls() {
        for value in [
            "",
            "example.com",
            "file:///tmp/app.exe",
            "javascript:alert(1)",
            "https://user:pass@example.com",
            "https://example.com/\n",
            "https://example.com/a b",
        ] {
            assert!(!valid_website_url(value), "{value:?}");
        }
        assert!(valid_website_url("http://localhost:3000/path?a=1&b=%22%26"));
        assert!(valid_website_url("https://example.com/"));
        for value in [
            "",
            "relative.exe",
            "/tmp/file.txt",
            "https://example.com/app.exe",
            "/tmp/app\0.exe",
        ] {
            assert!(!valid_application_path(value), "{value:?}");
        }
        for value in [
            "/Applications/Test & Space.app",
            "C:\\Program Files\\Test.exe",
            "\\\\server\\apps\\Test.lnk",
        ] {
            assert!(valid_application_path(value));
        }
    }
}
