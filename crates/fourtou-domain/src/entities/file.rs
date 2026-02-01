use bytes::Bytes;
use futures::Stream;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Type of a file system entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// A regular file.
    File,
    /// A directory.
    Directory,
}

/// A file or directory entry from a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// The name of the file or directory.
    pub name: String,
    /// The type of entry (file or directory).
    pub file_type: FileType,
}

impl FileEntry {
    /// Creates a new file entry.
    #[must_use]
    pub fn file(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            file_type: FileType::File,
        }
    }

    /// Creates a new directory entry.
    #[must_use]
    pub fn directory(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            file_type: FileType::Directory,
        }
    }

    /// Returns true if this entry is a file.
    #[must_use]
    pub fn is_file(&self) -> bool {
        self.file_type == FileType::File
    }

    /// Returns true if this entry is a directory.
    #[must_use]
    pub fn is_directory(&self) -> bool {
        self.file_type == FileType::Directory
    }
}

/// Metadata about a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetadata {
    /// The path to the file.
    pub path: String,
    /// The size in bytes, if known.
    pub size: Option<u64>,
    /// The MIME type, if known.
    pub content_type: Option<String>,
}

impl FileMetadata {
    /// Creates new file metadata.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            size: None,
            content_type: None,
        }
    }

    /// Sets the file size.
    #[must_use]
    pub const fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// Sets the content type.
    #[must_use]
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }
}

pin_project! {
    /// A stream of file content bytes.
    pub struct FileStream {
        #[pin]
        inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>,
    }
}

impl FileStream {
    /// Creates a new `FileStream` from a stream of bytes.
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    {
        Self {
            inner: Box::pin(stream),
        }
    }

    /// Creates a `FileStream` from static bytes (useful for testing).
    #[must_use]
    pub fn from_bytes(bytes: Bytes) -> Self {
        Self::new(futures::stream::once(async move { Ok(bytes) }))
    }

    /// Creates an empty `FileStream`.
    #[must_use]
    pub fn empty() -> Self {
        Self::new(futures::stream::empty())
    }
}

impl Stream for FileStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[test]
    fn should_create_file_entry_when_using_file_constructor() {
        let entry = FileEntry::file("test.txt");
        assert_eq!(entry.name, "test.txt");
        assert!(entry.is_file());
        assert!(!entry.is_directory());
    }

    #[test]
    fn should_create_directory_entry_when_using_directory_constructor() {
        let entry = FileEntry::directory("subdir");
        assert_eq!(entry.name, "subdir");
        assert!(entry.is_directory());
        assert!(!entry.is_file());
    }

    #[test]
    fn should_have_no_optional_fields_when_metadata_created_with_new() {
        let meta = FileMetadata::new("/path/to/file");
        assert_eq!(meta.path, "/path/to/file");
        assert!(meta.size.is_none());
        assert!(meta.content_type.is_none());
    }

    #[test]
    fn should_set_optional_fields_when_using_builder_methods() {
        let meta = FileMetadata::new("/path/to/file")
            .with_size(1024)
            .with_content_type("text/plain");

        assert_eq!(meta.path, "/path/to/file");
        assert_eq!(meta.size, Some(1024));
        assert_eq!(meta.content_type, Some("text/plain".to_string()));
    }

    #[tokio::test]
    async fn should_yield_bytes_when_stream_created_from_bytes() {
        let bytes = Bytes::from("hello world");
        let mut stream = FileStream::from_bytes(bytes.clone());

        let chunk = stream.next().await;
        assert!(chunk.is_some());
        assert_eq!(chunk.unwrap().unwrap(), bytes);

        let next = stream.next().await;
        assert!(next.is_none());
    }

    #[tokio::test]
    async fn should_yield_nothing_when_stream_is_empty() {
        let mut stream = FileStream::empty();
        let chunk = stream.next().await;
        assert!(chunk.is_none());
    }
}
