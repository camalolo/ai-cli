use patchkit::unified::parse_patch;

pub fn apply_patch(original: &str, diff: &str) -> Result<String, String> {
    let patch_lines: Vec<&str> = diff.lines().collect();
    let patch_bytes: Vec<&[u8]> = patch_lines.iter().map(|l| l.as_bytes()).collect();

    let patch = parse_patch(patch_bytes.into_iter()).map_err(|e| format!("Failed to parse patch: {}", e))?;
    let applied = patch.apply_exact(original.as_bytes()).map_err(|e| format!("Failed to apply patch: {}", e))?;

    String::from_utf8(applied).map_err(|e| format!("Invalid UTF-8 in patched content: {}", e))
}