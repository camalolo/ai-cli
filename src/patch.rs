use anyhow::{anyhow, Result};

pub fn apply_patch(original: &str, diff: &str) -> Result<String> {
    let patch = patch_apply::Patch::from_single(diff).map_err(|e| anyhow!("Failed to parse patch: {}", e))?;
    Ok(patch_apply::apply(original.to_string(), patch))
}