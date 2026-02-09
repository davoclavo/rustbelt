//! Builder pattern for creating RustAnalyzerish instances
//!
//! This module provides a builder pattern for initializing RustAnalyzerish
//! instances with workspace configuration, separating initialization concerns
//! from runtime operations.

use std::path::{Path, PathBuf};

use anyhow::Result;
use ra_ap_ide::AnalysisHost;
use ra_ap_ide_db::prime_caches;
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice, load_workspace_at};
use ra_ap_profile::StopWatch;
use ra_ap_project_model::{CargoConfig, ProjectManifest, RustLibSource};
use ra_ap_vfs::AbsPathBuf;
use tracing::{info, trace};

use super::analyzer::RustAnalyzerish;
use super::file_watcher::FileWatcher;
use super::utils::RustAnalyzerUtils;

/// Builder for creating configured RustAnalyzerish instances
#[derive(Debug)]
pub struct RustAnalyzerishBuilder {
    project_root: Option<PathBuf>,
    cargo_config: CargoConfig,
    load_config: LoadCargoConfig,
    enable_file_watching: bool,
}

impl Default for RustAnalyzerishBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RustAnalyzerishBuilder {
    /// Create a new builder with default configuration
    ///
    /// By default, file watching is disabled for fast CLI usage.
    /// Use `with_file_watching(true)` to enable it for long-running processes.
    pub fn new() -> Self {
        Self {
            project_root: None,
            cargo_config: CargoConfig {
                sysroot: Some(RustLibSource::Discover),
                all_targets: true,
                rustc_source: None,
                cfg_overrides: Default::default(),
                ..Default::default()
            },
            load_config: LoadCargoConfig {
                load_out_dirs_from_check: true,
                with_proc_macro_server: ProcMacroServerChoice::Sysroot,
                prefill_caches: false, // We handle this manually to add more cores
            },
            enable_file_watching: false,
        }
    }

    /// Enable or disable file watching
    ///
    /// File watching is useful for long-running processes (like MCP servers)
    /// that need to react to file changes. For one-shot CLI commands, it's
    /// not needed and can cause shutdown delays.
    pub fn with_file_watching(mut self, enable: bool) -> Self {
        self.enable_file_watching = enable;
        self
    }

    /// Set the workspace root directory
    fn with_workspace<P: AsRef<Path>>(mut self, workspace_root: P) -> Self {
        self.project_root = Some(workspace_root.as_ref().to_path_buf());
        self
    }

    /// Create a builder from a file path by finding its project root
    pub fn from_file<P: AsRef<Path>>(file_path: P) -> Result<Self> {
        let project_root = Self::find_project_root(file_path.as_ref())?;
        Ok(Self::new().with_workspace(project_root))
    }

    /// Configure cargo settings
    pub fn with_cargo_config(mut self, cargo_config: CargoConfig) -> Self {
        self.cargo_config = cargo_config;
        self
    }

    /// Configure load settings
    pub fn with_load_config(mut self, load_config: LoadCargoConfig) -> Self {
        self.load_config = load_config;
        self
    }

    /// Build the configured RustAnalyzerish instance
    pub fn build(self) -> Result<RustAnalyzerish> {
        let project_root = self
            .project_root
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No workspace root specified."))?;

        let abs_project_root = RustAnalyzerUtils::path_to_abs_path(&project_root)?;

        let (analysis_host, file_watcher) = self.load_workspace(&abs_project_root)?;

        Ok(RustAnalyzerish::new(analysis_host, file_watcher))
    }

    /// Find the project root by looking for Cargo.toml
    fn find_project_root(file_path: &Path) -> Result<PathBuf> {
        let path = if file_path.is_absolute() {
            info!(
                "Finding project root for absolute path: {}",
                file_path.display()
            );
            file_path.to_path_buf()
        } else {
            info!(
                "Finding project root for relative path: {}",
                file_path.display()
            );
            std::env::current_dir()?.join(file_path)
        };

        let abs_path = RustAnalyzerUtils::path_to_abs_path(&path)?;
        let root = ProjectManifest::discover_single(&abs_path)?;
        Ok(root.manifest_path().parent().to_path_buf().into())
    }

    /// Load workspace and return (AnalysisHost, FileWatcher)
    fn load_workspace(&self, abs_project_root: &AbsPathBuf) -> Result<(AnalysisHost, FileWatcher)> {
        info!("Loading workspace from: {}", abs_project_root);
        let mut stop_watch = StopWatch::start();

        let (db, vfs, _proc_macro) = load_workspace_at(
            abs_project_root.as_ref(),
            &self.cargo_config,
            &self.load_config,
            &|msg| {
                trace!("Workspace loading progress: {}", msg);
            },
        )?;

        // Create analysis host with the loaded database
        let mut host = AnalysisHost::with_database(db);

        let elapsed = stop_watch.elapsed();
        info!(
            "Load time: {:?}ms, memory allocated: {}MB",
            elapsed.time.as_millis(),
            elapsed.memory.allocated.megabytes() as u64
        );

        // Set up file watching (optionally)
        let mut file_watcher = FileWatcher::new();
        file_watcher.setup_file_watching(
            abs_project_root.clone(),
            vfs,
            &mut host,
            self.enable_file_watching,
        )?;

        // Prime caches with all available cores for better performance
        let threads = num_cpus::get_physical();
        prime_caches::parallel_prime_caches(host.raw_database(), threads, &|progress| {
            trace!("Cache priming progress: {:?}", progress);
        });

        let elapsed = stop_watch.elapsed();
        info!(
            "Cache priming time with {} cores: {:?}ms, total memory allocated: {}MB",
            threads,
            elapsed.time.as_millis(),
            elapsed.memory.allocated.megabytes() as u64
        );

        // Print all files in vfs for debugging
        for (file_id, vfs_path) in file_watcher.vfs().iter() {
            trace!("Loaded file in VFS: {:?} - {}", file_id, vfs_path);
        }

        Ok((host, file_watcher))
    }
}
