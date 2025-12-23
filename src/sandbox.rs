use std::path::PathBuf;
use std::sync::OnceLock;

pub static SANDBOX_ROOT: OnceLock<String> = OnceLock::new();

pub fn get_sandbox_root() -> &'static String {
    SANDBOX_ROOT.get_or_init(|| {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        dunce::canonicalize(&current_dir)
            .unwrap_or(current_dir)
            .to_string_lossy()
            .to_string()
    })
}