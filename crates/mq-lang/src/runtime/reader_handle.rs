//! Generic closeable reader behind `RuntimeValue::ReaderHandle`.
use crate::SharedCell;
use crate::io::{IoError, IoReader};
use std::borrow::Cow;
use std::io::Read;

/// What a [`ReaderHandle`] was opened from, used for `type_of`/display purposes only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HandleKind {
    #[cfg(feature = "file-io")]
    File,
    #[cfg(feature = "http")]
    Http,
}

impl HandleKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            #[cfg(feature = "file-io")]
            HandleKind::File => "file",
            #[cfg(feature = "http")]
            HandleKind::Http => "http",
        }
    }
}

/// Cloning the owning `RuntimeValue` shares the handle. The reader is dropped, and the source
/// closed, on [`close`](Self::close) or when the last reference goes away (including when a
/// coroutine holding it is closed or finishes).
pub(crate) struct ReaderHandle {
    kind: HandleKind,
    path: String,
    reader: SharedCell<Option<Box<dyn IoReader>>>,
}

fn closed() -> IoError {
    IoError::Other(Cow::Borrowed("reader handle is closed"))
}

fn read_err(err: std::io::Error) -> IoError {
    IoError::Other(Cow::Owned(err.to_string()))
}

impl ReaderHandle {
    pub(crate) fn new(kind: HandleKind, path: String, reader: Box<dyn IoReader>) -> Self {
        Self {
            kind,
            path,
            reader: SharedCell::new(Some(reader)),
        }
    }

    pub(crate) fn kind(&self) -> HandleKind {
        self.kind
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn is_open(&self) -> bool {
        #[cfg(not(feature = "sync"))]
        return self.reader.borrow().is_some();
        #[cfg(feature = "sync")]
        return self.reader.read().unwrap().is_some();
    }

    /// Closing an already closed handle is a no-op.
    pub(crate) fn close(&self) {
        #[cfg(not(feature = "sync"))]
        let mut reader = self.reader.borrow_mut();
        #[cfg(feature = "sync")]
        let mut reader = self.reader.write().unwrap();
        *reader = None;
    }

    fn with_reader<T>(&self, f: impl FnOnce(&mut dyn IoReader) -> Result<T, IoError>) -> Result<T, IoError> {
        #[cfg(not(feature = "sync"))]
        let mut reader = self.reader.borrow_mut();
        #[cfg(feature = "sync")]
        let mut reader = self.reader.write().unwrap();
        match reader.as_mut() {
            Some(reader) => f(reader.as_mut()),
            None => Err(closed()),
        }
    }

    /// Reads one line without its `\n`/`\r\n` terminator; `None` at end of file.
    pub(crate) fn read_line(&self) -> Result<Option<String>, IoError> {
        self.with_reader(|reader| {
            let mut buf = Vec::new();
            if reader.read_until(b'\n', &mut buf).map_err(read_err)? == 0 {
                return Ok(None);
            }
            if buf.ends_with(b"\n") {
                buf.pop();
                if buf.ends_with(b"\r") {
                    buf.pop();
                }
            }
            String::from_utf8(buf)
                .map(Some)
                .map_err(|_| IoError::Other(Cow::Borrowed("invalid UTF-8, use read_bytes instead")))
        })
    }

    /// Reads up to `n` bytes, fewer only at end of file; `None` when nothing is left.
    pub(crate) fn read_bytes(&self, n: usize) -> Result<Option<Vec<u8>>, IoError> {
        self.with_reader(|reader| {
            let mut buf = Vec::new();
            Read::take(reader, n as u64).read_to_end(&mut buf).map_err(read_err)?;
            Ok((!buf.is_empty()).then_some(buf))
        })
    }
}

impl std::fmt::Debug for ReaderHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReaderHandle")
            .field("kind", &self.kind.as_str())
            .field("path", &self.path)
            .field("open", &self.is_open())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::io::Cursor;

    #[cfg(feature = "file-io")]
    const TEST_KIND: HandleKind = HandleKind::File;
    #[cfg(all(not(feature = "file-io"), feature = "http"))]
    const TEST_KIND: HandleKind = HandleKind::Http;

    fn handle(content: &str) -> ReaderHandle {
        ReaderHandle::new(
            TEST_KIND,
            "test.txt".to_string(),
            Box::new(Cursor::new(content.as_bytes().to_vec())),
        )
    }

    fn lines(handle: &ReaderHandle) -> Vec<String> {
        std::iter::from_fn(|| handle.read_line().unwrap()).collect()
    }

    #[rstest]
    #[case::lf("a\nb\n", vec!["a", "b"])]
    #[case::crlf("a\r\nb\r\n", vec!["a", "b"])]
    #[case::no_trailing_newline("a\nb", vec!["a", "b"])]
    #[case::blank_lines("a\n\nb\n", vec!["a", "", "b"])]
    #[case::only_newline("\n", vec![""])]
    #[case::empty("", vec![])]
    fn read_line_splits_on_line_terminators(#[case] content: &str, #[case] expected: Vec<&str>) {
        assert_eq!(lines(&handle(content)), expected);
    }

    #[test]
    fn read_line_rejects_invalid_utf8() {
        let handle = ReaderHandle::new(
            TEST_KIND,
            "bin".to_string(),
            Box::new(Cursor::new(vec![0xff, 0xfe, b'\n'])),
        );
        assert!(handle.read_line().is_err());
    }

    #[test]
    fn read_bytes_returns_full_chunks_then_remainder_then_none() {
        let handle = handle("abcde");
        assert_eq!(handle.read_bytes(2).unwrap(), Some(b"ab".to_vec()));
        assert_eq!(handle.read_bytes(2).unwrap(), Some(b"cd".to_vec()));
        assert_eq!(handle.read_bytes(2).unwrap(), Some(b"e".to_vec()));
        assert_eq!(handle.read_bytes(2).unwrap(), None);
    }

    #[test]
    fn reads_after_close_fail_and_close_is_idempotent() {
        let handle = handle("a\n");
        assert!(handle.is_open());
        handle.close();
        handle.close();
        assert!(!handle.is_open());
        assert!(handle.read_line().is_err());
        assert!(handle.read_bytes(1).is_err());
    }
}

#[cfg(all(test, feature = "file-io"))]
mod release_tests {
    use crate::io::MemIo;
    use crate::{DefaultModuleResolver, Engine, Shared, null_input};
    use rstest::rstest;

    /// Open readers while the query's result is still alive, and after everything is dropped.
    fn open_readers(query: &str) -> (usize, usize) {
        let io = Shared::new(MemIo::default().with_file("a.txt", "x\ny\nz\n"));
        let mut engine = Engine::with_io(DefaultModuleResolver::default(), Shared::clone(&io));
        engine.load_builtin_module();
        let result = engine.eval(query, null_input().into_iter()).unwrap();
        let while_alive = io.open_readers();
        drop(result);
        drop(engine);
        (while_alive, io.open_readers())
    }

    #[rstest]
    #[case::handle_stays_open("open_file(\"a.txt\")", 1)]
    #[case::handle_closed("let f = open_file(\"a.txt\") | close(f) | f", 0)]
    #[case::stream_opened_eagerly("stream_lines(\"a.txt\")", 1)]
    #[case::stream_suspended("let s = stream_lines(\"a.txt\") | next(s) | s", 1)]
    #[case::stream_closed("let s = stream_lines(\"a.txt\") | next(s) | close(s) | s", 0)]
    #[case::closed_before_first_next("let s = stream_lines(\"a.txt\") | close(s) | s", 0)]
    #[case::stream_exhausted("let s = stream_lines(\"a.txt\") | collect(s) | s", 0)]
    #[case::chunks_exhausted("let s = stream_chunks(\"a.txt\", 2) | collect(s) | s", 0)]
    #[case::bytes_exhausted("let s = stream_bytes(\"a.txt\") | collect(s) | s", 0)]
    #[case::take_leaves_source_open("let s = stream_lines(\"a.txt\") | take(s, 1) | collect() | s", 1)]
    fn file_is_released_when_its_coroutine_is_closed_or_finished(#[case] query: &str, #[case] open_while_alive: usize) {
        assert_eq!(open_readers(query), (open_while_alive, 0), "query: {query}");
    }
}
