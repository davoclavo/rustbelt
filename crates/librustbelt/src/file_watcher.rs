//! File watching module for rust-analyzer integration
//!
//! This module handles file system watching and VFS synchronization,
//! keeping the analysis host updated with file changes.

use std::path::Path;

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, unbounded};
use ra_ap_ide::{AnalysisHost, FileId};
use ra_ap_ide_db::ChangeWithProcMacros;
use ra_ap_vfs::loader::{Handle, Message};
use ra_ap_vfs::{AbsPathBuf, Vfs, VfsPath};
use ra_ap_vfs_notify::NotifyHandle;
use tracing::{debug, error, trace};

/// File watching configuration and state
///
/// NOTE: Field order matters for drop order! The `vfs_handle` must be dropped
/// BEFORE `vfs_receiver` to avoid the VfsLoader thread panicking when trying
/// to send to a closed channel. Rust drops fields in declaration order.
#[derive(Debug)]
pub struct FileWatcher {
    vfs_handle: Option<NotifyHandle>,
    vfs_receiver: Option<Receiver<Message>>,
    vfs: Vfs,
}

impl Default for FileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl FileWatcher {
    /// Create a new file watcher
    pub fn new() -> Self {
        Self {
            vfs_handle: None,
            vfs_receiver: None,
            vfs: Vfs::default(),
        }
    }

    /// Set up file watching for the workspace
    ///
    /// If `enable_watching` is false, only the VFS is initialized without
    /// starting the file watcher thread. This is useful for one-shot CLI
    /// commands that don't need live file watching.
    pub fn setup_file_watching(
        &mut self,
        abs_project_root: AbsPathBuf,
        vfs: Vfs,
        _host: &mut AnalysisHost,
        enable_watching: bool,
    ) -> Result<()> {
        // Replace our VFS with the loaded workspace VFS
        self.vfs = vfs;

        if !enable_watching {
            tracing::info!(
                "File watching disabled, VFS initialized for: {}",
                abs_project_root
            );
            return Ok(());
        }

        tracing::info!(
            "Setting up file watching for workspace: {}",
            abs_project_root
        );

        // Create a channel for VFS loader messages
        let (sender, receiver) = unbounded::<Message>();

        // Start the file watcher using the same pattern as rust-analyzer
        let vfs_handle: NotifyHandle = Handle::spawn(sender);

        // Store the receiver and handle
        self.vfs_receiver = Some(receiver);
        self.vfs_handle = Some(vfs_handle);

        // Configure the VFS to watch the workspace files
        self.configure_vfs_watching(abs_project_root)?;

        Ok(())
    }

    /// Drain all pending messages from the file watcher and apply changes synchronously
    ///
    /// If file watching is disabled (no receiver), this is a no-op.
    pub fn drain_and_apply_changes(&mut self, host: &mut AnalysisHost) -> Result<()> {
        let Some(ref receiver) = self.vfs_receiver else {
            // File watching disabled, nothing to drain
            return Ok(());
        };

        // Process all pending messages from the file watcher
        while let Ok(message) = receiver.try_recv() {
            match message {
                Message::Progress {
                    n_total, n_done, ..
                } => {
                    trace!("File watching progress: {:?}/{:?}", n_done, n_total);
                }
                Message::Loaded { files } | Message::Changed { files } => {
                    debug!("Files changed: {} files", files.len());

                    // Process the loaded files
                    for (abs_path, contents) in files {
                        debug!("File changed: {:?}", abs_path);
                        let vfs_path: VfsPath = abs_path.to_path_buf().into();
                        self.vfs.set_file_contents(vfs_path, contents.clone());
                    }
                }
            }
        }

        // Apply all VFS changes to the analysis host
        let changed_files = self.vfs.take_changes();
        if changed_files.is_empty() {
            return Ok(());
        }
        let mut change = ChangeWithProcMacros::default();
        for (file_id, changed_file) in changed_files {
            let new_contents = match changed_file.change {
                ra_ap_vfs::Change::Create(v, _) | ra_ap_vfs::Change::Modify(v, _) => {
                    if let Ok(text) = std::str::from_utf8(&v) {
                        Some(text.to_owned())
                    } else {
                        None
                    }
                }
                ra_ap_vfs::Change::Delete => None,
            };
            change.change_file(file_id, new_contents);
        }

        host.apply_change(change);

        Ok(())
    }

    /// Configure VFS to watch workspace files
    fn configure_vfs_watching(&mut self, abs_project_root: AbsPathBuf) -> Result<()> {
        let Some(ref mut loader) = self.vfs_handle else {
            return Ok(());
        };

        debug!("Configuring VFS watching for: {}", abs_project_root);

        let config = ra_ap_vfs::loader::Config {
            load: vec![
                // Watch the entire project directory for changes
                ra_ap_vfs::loader::Entry::Directories(ra_ap_vfs::loader::Directories {
                    extensions: vec!["rs".to_string(), "toml".to_string()],
                    include: vec![abs_project_root.clone()],
                    exclude: vec![
                        abs_project_root.join("target"),
                        abs_project_root.join(".git"),
                    ],
                }),
            ],
            watch: vec![0], // Watch the first (and only) load entry
            version: 0,
        };

        // Set the configuration on the loader
        loader.set_config(config);

        debug!("VFS watching configuration set");
        Ok(())
    }

    pub fn get_file_id(&self, path: &Path) -> Result<FileId> {
        let vfs_path = Self::path_to_vfs_path(path)?;
        if let Some((file_id, _)) = self.vfs.file_id(&vfs_path) {
            debug!(
                "File found in VFS: {} with FileId: {:?}",
                path.display(),
                file_id
            );
            Ok(file_id)
        } else {
            error!("File not found in VFS: {}", path.display());
            Err(anyhow::anyhow!("File not found in VFS: {}", path.display()))
        }
    }

    /// Check if a file exists in the VFS
    pub fn file_exists(&self, file_id: FileId) -> bool {
        self.vfs.exists(file_id)
    }

    /// Get file path from file ID
    pub fn file_path(&self, file_id: FileId) -> Option<String> {
        if self.vfs.exists(file_id) {
            Some(self.vfs.file_path(file_id).to_string())
        } else {
            None
        }
    }

    /// Get a reference to the VFS
    pub fn vfs(&self) -> &Vfs {
        &self.vfs
    }

    /// Convert a PathBuf to VfsPath for VFS operations
    pub fn path_to_vfs_path(path: &Path) -> Result<VfsPath> {
        let abs_path = AbsPathBuf::assert_utf8(
            path.canonicalize()
                .with_context(|| format!("Failed to canonicalize path: {}", path.display()))?,
        );
        Ok(abs_path.into())
    }
}
