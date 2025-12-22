use once_cell::sync::Lazy;
use std::path::PathBuf;

pub static SANDBOX_ROOT: Lazy<String> = Lazy::new(|| {
    let path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
        .to_string_lossy()
        .to_string();

    // On Windows, canonicalize() adds \\?\ prefix, remove it for display
    #[cfg(target_os = "windows")]
    {
        if path.starts_with("\\\\?\\") {
            path[4..].to_string()
        } else {
            path
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        path
    }
});