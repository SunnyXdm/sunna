//! Validate a pre-existing system tree before interpreting its selection.
use anyhow::{ensure, Result};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

fn selected_version(root: &Path, link: &Path, target: &Path) -> bool {
    if link.parent() != Some(root)
        || !matches!(
            link.file_name().and_then(|n| n.to_str()),
            Some("current" | "current.new")
        )
        || target.parent() != Some(Path::new("versions"))
    {
        return false;
    }
    target
        .file_name()
        .and_then(|v| v.to_str())
        .is_some_and(|v| semver::Version::parse(v).is_ok())
}

pub fn validate(root: &Path) -> Result<()> {
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            let meta = std::fs::symlink_metadata(&path)?;
            ensure!(
                meta.uid() == 0,
                "system installation contains a file owned by another user"
            );
            if meta.is_symlink() {
                ensure!(
                    selected_version(root, &path, &std::fs::read_link(&path)?),
                    "unexpected system installation link"
                );
            } else {
                ensure!(
                    meta.mode() & 0o022 == 0 && (meta.is_dir() || meta.is_file()),
                    "system installation contains a writable or special file"
                );
                if meta.is_dir() {
                    dirs.push(path);
                }
            }
        }
    }
    Ok(())
}
