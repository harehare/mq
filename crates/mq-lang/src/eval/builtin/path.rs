use std::path::Path;

use super::Error;

/// Returns the final component of a path string, or the empty string if the path ends with a separator.
pub(super) fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Returns the parent directory of a path string, or "." if the path has no parent.
pub(super) fn dirname(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|p| {
            let s = p.to_string_lossy();
            if s.is_empty() { ".".to_owned() } else { s.into_owned() }
        })
        .unwrap_or_else(|| ".".to_owned())
}

/// Returns the extension of the file name, including the leading dot (e.g. `".txt"`).
/// Returns an empty string if there is no extension.
pub(super) fn extname(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default()
}

/// Returns the file name without the extension.
/// Returns an empty string if the path has no file name component.
pub(super) fn stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Joins a base path with a component path, returning the resulting path string.
pub(super) fn path_join(base: &str, component: &str) -> Result<String, Error> {
    let joined = Path::new(base).join(component);
    joined
        .to_str()
        .map(|s| s.to_owned())
        .ok_or_else(|| Error::Runtime("path_join: resulting path is not valid UTF-8".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("/path/to/file.txt", "file.txt")]
    #[case("/path/to/dir/", "dir")]
    #[case("file.txt", "file.txt")]
    #[case("/", "")]
    #[case(".", "")]
    fn test_basename(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(basename(input), expected);
    }

    #[rstest]
    #[case("/path/to/file.txt", "/path/to")]
    #[case("/path/to/dir/", "/path/to")]
    #[case("file.txt", ".")]
    #[case("/file.txt", "/")]
    #[case(".", ".")]
    fn test_dirname(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(dirname(input), expected);
    }

    #[rstest]
    #[case("/path/to/file.txt", ".txt")]
    #[case("/path/to/file.tar.gz", ".gz")]
    #[case("/path/to/file", "")]
    #[case("file.txt", ".txt")]
    #[case(".hidden", "")]
    fn test_extname(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(extname(input), expected);
    }

    #[rstest]
    #[case("/path/to/file.txt", "file")]
    #[case("/path/to/file.tar.gz", "file.tar")]
    #[case("/path/to/file", "file")]
    #[case("file.txt", "file")]
    #[case(".hidden", ".hidden")]
    fn test_stem(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(stem(input), expected);
    }

    #[rstest]
    #[case("/path/to", "file.txt", "/path/to/file.txt")]
    #[case("/path/to/", "file.txt", "/path/to/file.txt")]
    #[case(".", "file.txt", "./file.txt")]
    #[case("/a", "b/c", "/a/b/c")]
    fn test_path_join(#[case] base: &str, #[case] component: &str, #[case] expected: &str) {
        assert_eq!(path_join(base, component).unwrap(), expected);
    }
}
