use std::path::PathBuf;
use std::sync::OnceLock;

pub static SANDBOX_ROOT: OnceLock<String> = OnceLock::new();

pub fn get_sandbox_root() -> &'static String {
    SANDBOX_ROOT.get_or_init(|| {
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
    })
}