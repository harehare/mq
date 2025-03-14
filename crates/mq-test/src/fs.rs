use std::io::Write;
use std::{fs::File, path::PathBuf};

pub type TempDir = PathBuf;
pub type TempFile = PathBuf;

pub fn create_file(name: &str, content: &str) -> (TempDir, TempFile) {
    let temp_dir = std::env::temp_dir();
    let temp_file_path = temp_dir.join(name);
    let mut file = File::create(&temp_file_path).expect("Failed to create temp file");
    file.write_all(content.as_bytes())
        .expect("Failed to write to temp file");

    (temp_dir, temp_file_path)
}
