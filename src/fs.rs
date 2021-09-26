//! Contains the structs and traits that define a filesystem backend.
//!
//! You only need this if you are going to implement your own
//! filesystem backend. Otherwise, just use 'LocalFs' or 'MemFs'.
//!
use std::fmt::Debug;
use std::io::SeekFrom;
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::{future, Future, Stream, TryFutureExt};
use http::StatusCode;

use crate::davpath::DavPath;

macro_rules! notimplemented {
    ($method:expr) => {
        Err(FsError::NotImplemented)
    };
}

macro_rules! notimplemented_fut {
    ($method:expr) => {
        Box::pin(future::ready(Err(FsError::NotImplemented)))
    };
}

/// Errors generated by a filesystem implementation.
///
/// These are more result-codes than errors, really.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FsError {
    /// Operation not implemented (501)
    NotImplemented,
    /// Something went wrong (500)
    GeneralFailure,
    /// tried to create something, but it existed (405 / 412) (yes, 405. RFC4918 says so)
    Exists,
    /// File / Directory not found (404)
    NotFound,
    /// Not allowed (403)
    Forbidden,
    /// Out of space (507)
    InsufficientStorage,
    /// Symbolic link loop detected (ELOOP) (508)
    LoopDetected,
    /// The path is too long (ENAMETOOLONG) (414)
    PathTooLong,
    /// The file being PUT is too large (413)
    TooLarge,
    /// Trying to MOVE over a mount boundary (EXDEV) (502)
    IsRemote,
}
/// The Result type.
pub type FsResult<T> = std::result::Result<T, FsError>;

/// A webdav property.
#[derive(Debug, Clone)]
pub struct DavProp {
    /// Name of the property.
    pub name:      String,
    /// XML prefix.
    pub prefix:    Option<String>,
    /// XML namespace.
    pub namespace: Option<String>,
    /// Value of the property as raw XML.
    pub xml:       Option<Vec<u8>>,
}

/// Future returned by almost all of the DavFileSystem methods.
pub type FsFuture<'a, T> = Pin<Box<dyn Future<Output = FsResult<T>> + Send + 'a>>;
/// Convenience alias for a boxed Stream.
pub type FsStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

/// Used as argument to the read_dir() method.
/// It is:
///
/// - an optimization hint (the implementation may call metadata() and
///   store the result in the returned directory entry)
/// - a way to get metadata instead of symlink_metadata from
///   the directory entry.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadDirMeta {
    /// DavDirEntry.metadata() behaves as metadata()
    Data,
    /// DavDirEntry.metadata() behaves as symlink_metadata()
    DataSymlink,
    /// No optimizations, otherwise like DataSymlink.
    None,
}

/// The trait that defines a filesystem.
pub trait DavFileSystem: Sync + Send + BoxCloneFs {
    /// Open a file.
    fn open<'a>(&'a self, path: &'a DavPath, options: OpenOptions) -> FsFuture<Box<dyn DavFile>>;

    /// Perform read_dir.
    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        meta: ReadDirMeta,
    ) -> FsFuture<FsStream<Box<dyn DavDirEntry>>>;

    /// Return the metadata of a file or directory.
    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<Box<dyn DavMetaData>>;

    /// Return the metadata of a file, directory or symbolic link.
    ///
    /// Differs from metadata() that if the path is a symbolic link,
    /// it return the metadata for the link itself, not for the thing
    /// it points to.
    ///
    /// The default implementation returns FsError::NotImplemented.
    #[allow(unused_variables)]
    fn symlink_metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<Box<dyn DavMetaData>> {
        self.metadata(path)
    }

    /// Create a directory.
    ///
    /// The default implementation returns FsError::NotImplemented.
    #[allow(unused_variables)]
    fn create_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<()> {
        notimplemented_fut!("create_dir")
    }

    /// Remove a directory.
    ///
    /// The default implementation returns FsError::NotImplemented.
    #[allow(unused_variables)]
    fn remove_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<()> {
        notimplemented_fut!("remove_dir")
    }

    /// Remove a file.
    ///
    /// The default implementation returns FsError::NotImplemented.
    #[allow(unused_variables)]
    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<()> {
        notimplemented_fut!("remove_file")
    }

    /// Rename a file or directory.
    ///
    /// Source and destination must be the same type (file/dir).
    /// If the destination already exists and is a file, it
    /// should be replaced. If it is a directory it should give
    /// an error.
    ///
    /// The default implementation returns FsError::NotImplemented.
    #[allow(unused_variables)]
    fn rename<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<()> {
        notimplemented_fut!("rename")
    }

    /// Copy a file
    ///
    /// Should also copy the DAV properties, if properties
    /// are implemented.
    ///
    /// The default implementation returns FsError::NotImplemented.
    #[allow(unused_variables)]
    fn copy<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<()> {
        notimplemented_fut!("copy")
    }

    /// Set the access time of a file / directory.
    ///
    /// The default implementation returns FsError::NotImplemented.
    #[doc(hidden)]
    #[allow(unused_variables)]
    fn set_accessed<'a>(&'a self, path: &'a DavPath, tm: SystemTime) -> FsFuture<()> {
        notimplemented_fut!("set_accessed")
    }

    /// Set the modified time of a file / directory.
    ///
    /// The default implementation returns FsError::NotImplemented.
    #[doc(hidden)]
    #[allow(unused_variables)]
    fn set_modified<'a>(&'a self, path: &'a DavPath, tm: SystemTime) -> FsFuture<()> {
        notimplemented_fut!("set_mofified")
    }

    /// Indicator that tells if this filesystem driver supports DAV properties.
    ///
    /// The default implementation returns `false`.
    #[allow(unused_variables)]
    fn have_props<'a>(&'a self, path: &'a DavPath) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(future::ready(false))
    }

    /// Patch the DAV properties of a node (add/remove props)
    ///
    /// The default implementation returns FsError::NotImplemented.
    #[allow(unused_variables)]
    fn patch_props<'a>(
        &'a self,
        path: &'a DavPath,
        patch: Vec<(bool, DavProp)>,
    ) -> FsFuture<Vec<(StatusCode, DavProp)>>
    {
        notimplemented_fut!("patch_props")
    }

    /// List/get the DAV properties of a node.
    ///
    /// The default implementation returns FsError::NotImplemented.
    #[allow(unused_variables)]
    fn get_props<'a>(&'a self, path: &'a DavPath, do_content: bool) -> FsFuture<Vec<DavProp>> {
        notimplemented_fut!("get_props")
    }

    /// Get one specific named property of a node.
    ///
    /// The default implementation returns FsError::NotImplemented.
    #[allow(unused_variables)]
    fn get_prop<'a>(&'a self, path: &'a DavPath, prop: DavProp) -> FsFuture<Vec<u8>> {
        notimplemented_fut!("get_prop`")
    }

    /// Get quota of this filesystem (used/total space).
    ///
    /// The first value returned is the amount of space used,
    /// the second optional value is the total amount of space
    /// (used + available).
    ///
    /// The default implementation returns FsError::NotImplemented.
    #[allow(unused_variables)]
    fn get_quota<'a>(&'a self) -> FsFuture<(u64, Option<u64>)> {
        notimplemented_fut!("get_quota`")
    }
}

// BoxClone trait.
#[doc(hidden)]
pub trait BoxCloneFs {
    fn box_clone(&self) -> Box<dyn DavFileSystem>;
}

// generic Clone, calls implementation-specific box_clone().
impl Clone for Box<dyn DavFileSystem> {
    fn clone(&self) -> Box<dyn DavFileSystem> {
        self.box_clone()
    }
}

// implementation-specific clone.
#[doc(hidden)]
impl<FS: Clone + DavFileSystem + 'static> BoxCloneFs for FS {
    fn box_clone(&self) -> Box<dyn DavFileSystem> {
        Box::new((*self).clone())
    }
}

/// One directory entry (or child node).
pub trait DavDirEntry: Send + Sync {
    /// Name of the entry.
    fn name(&self) -> Vec<u8>;

    /// Metadata of the entry.
    fn metadata<'a>(&'a self) -> FsFuture<Box<dyn DavMetaData>>;

    /// Default implementation of `is_dir` just returns `metadata()?.is_dir()`.
    /// Implementations can override this if their `metadata()` method is
    /// expensive and there is a cheaper way to provide the same info
    /// (e.g. dirent.d_type in unix filesystems).
    fn is_dir<'a>(&'a self) -> FsFuture<bool> {
        Box::pin(self.metadata().and_then(|meta| future::ok(meta.is_dir())))
    }

    /// Likewise. Default: `!is_dir()`.
    fn is_file<'a>(&'a self) -> FsFuture<bool> {
        Box::pin(self.metadata().and_then(|meta| future::ok(meta.is_file())))
    }

    /// Likewise. Default: `false`.
    fn is_symlink<'a>(&'a self) -> FsFuture<bool> {
        Box::pin(self.metadata().and_then(|meta| future::ok(meta.is_symlink())))
    }
}

/// A `DavFile` is the equivalent of `std::fs::File`, should be
/// readable/writeable/seekable, and be able to return its metadata.
pub trait DavFile: Debug + Send + Sync {
    fn metadata<'a>(&'a mut self) -> FsFuture<Box<dyn DavMetaData>>;
    fn write_buf<'a>(&'a mut self, buf: Box<dyn bytes::Buf + Send>) -> FsFuture<()>;
    fn write_bytes<'a>(&'a mut self, buf: bytes::Bytes) -> FsFuture<()>;
    fn read_bytes<'a>(&'a mut self, count: usize) -> FsFuture<bytes::Bytes>;
    fn seek<'a>(&'a mut self, pos: SeekFrom) -> FsFuture<u64>;
    fn flush<'a>(&'a mut self) -> FsFuture<()>;
}

/// File metadata. Basically type, length, and some timestamps.
pub trait DavMetaData: Debug + BoxCloneMd + Send + Sync {
    /// Size of the file.
    fn len(&self) -> u64;
    /// `Modified` timestamp.
    fn modified(&self) -> FsResult<SystemTime>;
    /// File or directory (aka collection).
    fn is_dir(&self) -> bool;

    /// Simplistic implementation of `etag()`
    ///
    /// Returns a simple etag that basically is `\<length\>-\<timestamp_in_ms\>`
    /// with the numbers in hex. Enough for most implementations.
    fn etag(&self) -> Option<String> {
        if let Ok(t) = self.modified() {
            if let Ok(t) = t.duration_since(UNIX_EPOCH) {
                let t = t.as_secs() * 1000000 + t.subsec_nanos() as u64 / 1000;
                let tag = if self.is_file() && self.len() > 0 {
                    format!("{:x}-{:x}", self.len(), t)
                } else {
                    format!("{:x}", t)
                };
                return Some(tag);
            }
        }
        None
    }

    /// Is this a file and not a directory. Default: `!s_dir()`.
    fn is_file(&self) -> bool {
        !self.is_dir()
    }

    /// Is this a symbolic link. Default: false.
    fn is_symlink(&self) -> bool {
        false
    }

    /// Last access time. Default: `FsError::NotImplemented`.
    fn accessed(&self) -> FsResult<SystemTime> {
        notimplemented!("access time")
    }

    /// Creation time. Default: `FsError::NotImplemented`.
    fn created(&self) -> FsResult<SystemTime> {
        notimplemented!("creation time")
    }

    /// Inode change time (ctime). Default: `FsError::NotImplemented`.
    fn status_changed(&self) -> FsResult<SystemTime> {
        notimplemented!("status change time")
    }

    /// Is file executable (unix: has "x" mode bit). Default: `FsError::NotImplemented`.
    fn executable(&self) -> FsResult<bool> {
        notimplemented!("executable")
    }
}

// generic Clone, calls implementation-specific box_clone().
impl Clone for Box<dyn DavMetaData> {
    fn clone(&self) -> Box<dyn DavMetaData> {
        self.box_clone()
    }
}

// BoxCloneMd trait.
#[doc(hidden)]
pub trait BoxCloneMd {
    fn box_clone(&self) -> Box<dyn DavMetaData>;
}

// implementation-specific clone.
#[doc(hidden)]
impl<MD: Clone + DavMetaData + 'static> BoxCloneMd for MD {
    fn box_clone(&self) -> Box<dyn DavMetaData> {
        Box::new((*self).clone())
    }
}

/// OpenOptions for `open()`.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenOptions {
    /// open for reading
    pub read:       bool,
    /// open for writing
    pub write:      bool,
    /// open in write-append mode
    pub append:     bool,
    /// truncate file first when writing
    pub truncate:   bool,
    /// create file if it doesn't exist
    pub create:     bool,
    /// must create new file, fail if it already exists.
    pub create_new: bool,
    /// write file total size
    pub size: Option<u64>,
}

impl OpenOptions {
    #[allow(dead_code)]
    pub(crate) fn new() -> OpenOptions {
        OpenOptions {
            read:       false,
            write:      false,
            append:     false,
            truncate:   false,
            create:     false,
            create_new: false,
            size:       None,
        }
    }

    pub(crate) fn read() -> OpenOptions {
        OpenOptions {
            read:       true,
            write:      false,
            append:     false,
            truncate:   false,
            create:     false,
            create_new: false,
            size:       None,
        }
    }

    pub(crate) fn write() -> OpenOptions {
        OpenOptions {
            read:       false,
            write:      true,
            append:     false,
            truncate:   false,
            create:     false,
            create_new: false,
            size:       None,
        }
    }
}

impl std::error::Error for FsError {
    fn description(&self) -> &str {
        "DavFileSystem error"
    }
    fn cause(&self) -> Option<&dyn std::error::Error> {
        None
    }
}

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
