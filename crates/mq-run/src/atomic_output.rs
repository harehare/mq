//! Atomic writes for `-o`/`--output`: write to a temp file next to the
//! target, then `fsync` + `rename` in [`OutputSink::finish`], so a crash or
//! full disk mid-write can't leave a truncated file at the destination.

use miette::IntoDiagnostic;
use miette::miette;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum AtomicOutput {
    /// Atomic for a missing or regular-file target; direct write for pipes/devices.
    #[default]
    Auto,
    /// Always write atomically, even over pipes/devices.
    Always,
    /// Always write directly to the target path, truncating it up front.
    Never,
}

/// Call [`OutputSink::finish`] once done writing.
pub(crate) enum OutputSink {
    Stdout(io::StdoutLock<'static>),
    BufferedStdout(BufWriter<io::StdoutLock<'static>>),
    Direct(BufWriter<fs::File>),
    Atomic {
        writer: BufWriter<fs::File>,
        temp_path: PathBuf,
        target_path: PathBuf,
    },
}

impl Write for OutputSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            OutputSink::Stdout(w) => w.write(buf),
            OutputSink::BufferedStdout(w) => w.write(buf),
            OutputSink::Direct(w) => w.write(buf),
            OutputSink::Atomic { writer, .. } => writer.write(buf),
        }
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        match self {
            OutputSink::Stdout(w) => w.write_all(buf),
            OutputSink::BufferedStdout(w) => w.write_all(buf),
            OutputSink::Direct(w) => w.write_all(buf),
            OutputSink::Atomic { writer, .. } => writer.write_all(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            OutputSink::Stdout(w) => w.flush(),
            OutputSink::BufferedStdout(w) => w.flush(),
            OutputSink::Direct(w) => w.flush(),
            OutputSink::Atomic { writer, .. } => writer.flush(),
        }
    }
}

impl OutputSink {
    pub(crate) fn open(output_file: &Option<PathBuf>, atomic: AtomicOutput, unbuffered: bool) -> miette::Result<Self> {
        match output_file {
            Some(path) => Self::open_file(path, atomic),
            None if unbuffered => Ok(OutputSink::Stdout(io::stdout().lock())),
            None => Ok(OutputSink::BufferedStdout(BufWriter::new(io::stdout().lock()))),
        }
    }

    fn open_file(path: &Path, atomic: AtomicOutput) -> miette::Result<Self> {
        let use_atomic = match atomic {
            AtomicOutput::Never => false,
            AtomicOutput::Always => true,
            AtomicOutput::Auto => Self::target_supports_atomic(path)?,
        };

        if !use_atomic {
            let file = fs::File::create(path).into_diagnostic()?;
            return Ok(OutputSink::Direct(BufWriter::new(file)));
        }

        let (file, temp_path) = Self::create_temp_file(path)?;
        Ok(OutputSink::Atomic {
            writer: BufWriter::new(file),
            temp_path,
            target_path: path.to_path_buf(),
        })
    }

    /// FIFOs, sockets, and device files can't safely be replaced via `rename`.
    fn target_supports_atomic(path: &Path) -> miette::Result<bool> {
        match fs::symlink_metadata(path) {
            Ok(meta) => {
                let file_type = meta.file_type();
                Ok(file_type.is_file() || file_type.is_symlink())
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(true),
            Err(e) => Err(miette!(e)),
        }
    }

    /// Same directory as `target`, so the later `rename` stays on one filesystem.
    fn create_temp_file(target: &Path) -> miette::Result<(fs::File, PathBuf)> {
        let dir = match target.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        };
        let file_name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "output".to_string());

        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let pid = std::process::id();

        for _ in 0..64 {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default();
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let candidate = dir.join(format!(".{file_name}.mq-tmp-{pid:x}-{nanos:x}-{unique:x}"));

            match fs::OpenOptions::new().write(true).create_new(true).open(&candidate) {
                Ok(file) => return Ok((file, candidate)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(miette!(
                        "failed to create a temporary file for atomic write in {}: {e}",
                        dir.display()
                    ));
                }
            }
        }

        Err(miette!(
            "failed to create a unique temporary file for atomic write in {}",
            dir.display()
        ))
    }

    pub(crate) fn finish(self) -> miette::Result<()> {
        match self {
            OutputSink::Stdout(_) => Ok(()),
            OutputSink::BufferedStdout(mut w) => Self::ignore_broken_pipe(w.flush()),
            OutputSink::Direct(mut w) => Self::ignore_broken_pipe(w.flush()),
            OutputSink::Atomic {
                writer,
                temp_path,
                target_path,
            } => {
                let result = Self::publish_atomic(writer, &temp_path, &target_path);
                if result.is_err() {
                    let _ = fs::remove_file(&temp_path);
                }
                result
            }
        }
    }

    fn publish_atomic(mut writer: BufWriter<fs::File>, temp_path: &Path, target_path: &Path) -> miette::Result<()> {
        writer.flush().into_diagnostic()?;
        let file = writer
            .into_inner()
            .map_err(|e| miette!("failed to flush temporary output file: {}", e.error()))?;
        file.sync_all().into_diagnostic()?;
        drop(file);

        if let Ok(meta) = fs::metadata(target_path) {
            let _ = fs::set_permissions(temp_path, meta.permissions());
        }

        fs::rename(temp_path, target_path).into_diagnostic()?;
        Ok(())
    }

    fn ignore_broken_pipe(result: io::Result<()>) -> miette::Result<()> {
        match result {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            Err(e) => Err(miette!(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use scopeguard::defer;

    fn temp_dir(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "mq-atomic-output-test-{name}-{}-{unique:x}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("failed to create test directory");
        dir
    }

    #[rstest]
    #[case::auto(AtomicOutput::Auto)]
    #[case::always(AtomicOutput::Always)]
    fn test_atomic_write_new_file(#[case] mode: AtomicOutput) {
        let dir = temp_dir("new-file");
        defer! { let _ = fs::remove_dir_all(&dir); }
        let target = dir.join("out.md");

        let mut sink = OutputSink::open(&Some(target.clone()), mode, false).unwrap();
        sink.write_all(b"hello").unwrap();
        sink.finish().unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
        let entries: Vec<_> = fs::read_dir(&dir).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_atomic_write_overwrites_existing_file() {
        let dir = temp_dir("overwrite");
        defer! { let _ = fs::remove_dir_all(&dir); }
        let target = dir.join("out.md");
        fs::write(&target, "old content that is longer than new").unwrap();

        let mut sink = OutputSink::open(&Some(target.clone()), AtomicOutput::Auto, false).unwrap();
        sink.write_all(b"new").unwrap();
        sink.finish().unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    }

    #[test]
    fn test_never_mode_writes_directly() {
        let dir = temp_dir("never");
        defer! { let _ = fs::remove_dir_all(&dir); }
        let target = dir.join("out.md");

        let mut sink = OutputSink::open(&Some(target.clone()), AtomicOutput::Never, false).unwrap();
        sink.write_all(b"direct").unwrap();
        sink.finish().unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "direct");
        let entries: Vec<_> = fs::read_dir(&dir).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_finish_error_cleans_up_temp_file() {
        let dir = temp_dir("cleanup");
        defer! { let _ = fs::remove_dir_all(&dir); }
        // Rename onto a directory fails.
        let target = dir.join("out_dir");
        fs::create_dir(&target).unwrap();

        let mut sink = OutputSink::open(&Some(target.clone()), AtomicOutput::Always, false).unwrap();
        sink.write_all(b"data").unwrap();
        assert!(sink.finish().is_err());

        let leftover: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name() != "out_dir")
            .collect();
        assert!(leftover.is_empty(), "temp file should be cleaned up on failure");
    }

    #[cfg(unix)]
    #[test]
    fn test_atomic_write_preserves_existing_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("permissions");
        defer! { let _ = fs::remove_dir_all(&dir); }
        let target = dir.join("out.md");
        fs::write(&target, "old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();

        let mut sink = OutputSink::open(&Some(target.clone()), AtomicOutput::Auto, false).unwrap();
        sink.write_all(b"new").unwrap();
        sink.finish().unwrap();

        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }

    #[cfg(unix)]
    #[test]
    fn test_auto_mode_falls_back_to_direct_write_for_socket() {
        use std::os::unix::fs::FileTypeExt;
        use std::os::unix::net::UnixListener;

        let dir = temp_dir("socket");
        defer! { let _ = fs::remove_dir_all(&dir); }
        let target = dir.join("out.sock");

        let _listener = UnixListener::bind(&target).expect("failed to bind unix socket");
        assert!(fs::symlink_metadata(&target).unwrap().file_type().is_socket());
        assert!(!OutputSink::target_supports_atomic(&target).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn test_always_mode_skips_special_file_check() {
        use std::os::unix::fs::FileTypeExt;
        use std::os::unix::net::UnixListener;

        let dir = temp_dir("socket-always");
        defer! { let _ = fs::remove_dir_all(&dir); }
        let target = dir.join("out.sock");
        let _listener = UnixListener::bind(&target).expect("failed to bind unix socket");

        // `always` skips the special-file check.
        let mut sink = OutputSink::open(&Some(target.clone()), AtomicOutput::Always, false).unwrap();
        sink.write_all(b"data").unwrap();
        sink.finish().unwrap();

        assert!(!fs::symlink_metadata(&target).unwrap().file_type().is_socket());
        assert_eq!(fs::read_to_string(&target).unwrap(), "data");
    }
}
