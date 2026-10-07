//! Small filesystem helpers. None of them follow links, so a junction placed
//! inside a cache folder can never steer a scan or a delete somewhere else.

use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::Path;

const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// True for symlinks and junctions, which this app never walks through.
pub fn is_reparse(meta: &fs::Metadata) -> bool {
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

/// Whether the entry is a directory by its attribute bit. `Metadata::is_dir`
/// answers false for a junction, but removing one needs `remove_dir`.
pub fn has_dir_attribute(meta: &fs::Metadata) -> bool {
    meta.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub bytes: u64,
    pub files: u64,
}

/// Total size of every regular file below `root`.
pub fn dir_size(root: &Path) -> Size {
    let mut total = Size::default();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if is_reparse(&meta) {
                continue;
            }
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total.bytes += meta.len();
                total.files += 1;
            }
        }
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn sums_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        fs::create_dir_all(&nested).unwrap();
        fs::File::create(dir.path().join("one.bin"))
            .unwrap()
            .write_all(&[0u8; 10])
            .unwrap();
        fs::File::create(nested.join("two.bin"))
            .unwrap()
            .write_all(&[0u8; 25])
            .unwrap();

        let size = dir_size(dir.path());
        assert_eq!(
            size,
            Size {
                bytes: 35,
                files: 2
            }
        );
    }

    #[test]
    fn missing_folder_is_empty() {
        let size = dir_size(Path::new(r"Z:\definitely\not\here"));
        assert_eq!(size, Size::default());
    }
}
