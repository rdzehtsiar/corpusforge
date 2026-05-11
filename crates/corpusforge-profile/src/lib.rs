// SPDX-License-Identifier: Apache-2.0

//! Deterministic fixture profile compiler for CorpusForge.

use std::fs;
use std::path::{Component, Path};

use corpusforge_cff::{ProfileFile, ProfilePack};
use corpusforge_core::{CorpusForgeError, Result};

/// Returns the crate identifier used in workspace smoke tests.
pub const fn crate_name() -> &'static str {
    "corpusforge-profile"
}

/// Compiles a local file or directory into a deterministic `.cff` profile pack.
pub fn compile_path(path: impl AsRef<Path>) -> Result<ProfilePack> {
    let path = path.as_ref();
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CorpusForgeError::invalid_argument(
                "input path does not exist; provide an existing file or directory",
            )
        } else {
            CorpusForgeError::from(error)
        }
    })?;

    let mut files = if metadata.is_file() {
        vec![compile_file(path, stable_file_name(path)?)?]
    } else if metadata.is_dir() {
        compile_directory(path)?
    } else {
        return Err(CorpusForgeError::invalid_argument(
            "input path is not a regular file or directory; provide a fixture file or directory",
        ));
    };

    if files.is_empty() {
        return Err(CorpusForgeError::invalid_profile(
            "profile input contains no regular files; add at least one fixture file",
        ));
    }

    files.sort_by(|left, right| left.path().cmp(right.path()));
    ProfilePack::new(files)
}

fn compile_directory(root: &Path) -> Result<Vec<ProfileFile>> {
    let mut files = Vec::new();
    collect_directory(root, root, &mut files)?;
    Ok(files)
}

fn collect_directory(root: &Path, directory: &Path, files: &mut Vec<ProfileFile>) -> Result<()> {
    let mut entries = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            collect_directory(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path.strip_prefix(root).map_err(|_| {
                CorpusForgeError::determinism_violation(
                    "collected file is outside the profile root; retry with a stable local path",
                )
            })?;
            files.push(compile_file(&path, stable_relative_path(relative)?)?);
        } else {
            return Err(CorpusForgeError::invalid_argument(
                "profile input contains an unsupported directory entry; use only regular files and directories",
            ));
        }
    }

    Ok(())
}

fn compile_file(path: &Path, stable_path: String) -> Result<ProfileFile> {
    let bytes = fs::read(path)?;
    ProfileFile::new(stable_path, bytes)
}

fn stable_file_name(path: &Path) -> Result<String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            CorpusForgeError::invalid_profile(
                "input file name is not valid UTF-8; use a stable UTF-8 file name",
            )
        })?;

    Ok(file_name.to_owned())
}

fn stable_relative_path(path: &Path) -> Result<String> {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value.to_str().ok_or_else(|| {
                    CorpusForgeError::invalid_profile(
                        "profile relative path is not valid UTF-8; use stable UTF-8 path components",
                    )
                })?;
                components.push(value);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CorpusForgeError::invalid_profile(
                    "profile relative path contains an unstable component; use normalized relative fixture paths",
                ));
            }
        }
    }

    if components.is_empty() {
        return Err(CorpusForgeError::invalid_profile(
            "profile relative path is empty; use a fixture file below the input directory",
        ));
    }

    Ok(components.join("/"))
}

#[cfg(test)]
mod tests {
    use super::{compile_path, crate_name};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn exposes_crate_name() {
        assert_eq!(crate_name(), "corpusforge-profile");
    }

    #[test]
    fn compiles_single_file_with_stable_path_and_raw_bytes() {
        let temp = TestDir::new("single-file");
        temp.write("fixture.txt", b"alpha\nbeta");

        let pack = compile_path(temp.path().join("fixture.txt")).expect("file should compile");

        assert_eq!(pack.files().len(), 1);
        assert_eq!(pack.files()[0].path(), "fixture.txt");
        assert_eq!(pack.files()[0].bytes(), b"alpha\nbeta");
    }

    #[test]
    fn compiles_directory_with_sorted_slash_paths() {
        let temp = TestDir::new("directory-sort");
        temp.write("zeta.txt", b"z");
        temp.write("nested/beta.txt", b"b");
        temp.write("nested/alpha.txt", b"a");
        temp.write("alpha/leaf.txt", b"leaf");

        let pack = compile_path(temp.path()).expect("directory should compile");
        let paths: Vec<_> = pack
            .files()
            .iter()
            .map(|file| file.path().to_owned())
            .collect();

        assert_eq!(
            paths,
            vec![
                "alpha/leaf.txt",
                "nested/alpha.txt",
                "nested/beta.txt",
                "zeta.txt"
            ]
        );
    }

    #[test]
    fn empty_directory_rejects_cleanly() {
        let temp = TestDir::new("empty-directory");

        let error = compile_path(temp.path()).expect_err("empty input should fail");

        assert_eq!(error.category(), "invalid_profile");
        assert!(error.to_string().contains("no regular files"));
    }

    #[test]
    fn missing_path_rejects_cleanly() {
        let temp = TestDir::new("missing-path");

        let error = compile_path(temp.path().join("missing.txt")).expect_err("missing should fail");

        assert_eq!(error.category(), "invalid_argument");
        assert!(error.to_string().contains("does not exist"));
    }

    #[test]
    fn preserves_non_utf8_file_contents() {
        let temp = TestDir::new("raw-bytes");
        let bytes = [0xff, 0x00, 0x80, b'a', b'\n'];
        temp.write("bytes.bin", &bytes);

        let pack = compile_path(temp.path()).expect("directory should compile");

        assert_eq!(pack.files().len(), 1);
        assert_eq!(pack.files()[0].path(), "bytes.bin");
        assert_eq!(pack.files()[0].bytes(), bytes);
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::current_dir()
                .expect("current directory should be available")
                .join("target")
                .join("corpusforge-profile-tests")
                .join(format!("{}-{id}-{}", std::process::id(), name));

            if path.exists() {
                fs::remove_dir_all(&path).expect("stale test directory should be removable");
            }
            fs::create_dir_all(&path).expect("test directory should be created");

            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write(&self, relative: &str, bytes: &[u8]) {
            let path = self.path.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("test parent directory should be created");
            }
            fs::write(path, bytes).expect("test fixture should be written");
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
