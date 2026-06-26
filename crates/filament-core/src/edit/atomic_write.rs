//! Crash-safe writes: stage into a temp file in the same directory, then rename
//! over the target (an atomic operation on the same filesystem).

use std::io::Write;
use std::path::Path;

use crate::error::CoreError;

pub fn atomic_write(path: &Path, contents: &str) -> Result<(), CoreError> {
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };

    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(|e| CoreError::io(path, e))?;
    tmp.write_all(contents.as_bytes())
        .map_err(|e| CoreError::io(path, e))?;
    tmp.flush().map_err(|e| CoreError::io(path, e))?;
    tmp.persist(path)
        .map_err(|e| CoreError::io(path, e.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.md");
        atomic_write(&path, "first").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");
        atomic_write(&path, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
    }
}
