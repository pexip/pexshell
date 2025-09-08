#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::redundant_pub_crate)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::significant_drop_in_scrutinee)]

pub mod fs;
pub mod future;
pub mod googletest;
pub mod logging;

use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use fs::{Configurer, RootSchemaBuilder, SchemaBuilder};
use log::{info, warn, LevelFilter};
use logging::{TestLogger, TestLoggerContext, TestLoggerPermit};
use parking_lot::Mutex;
use uuid::Uuid;

#[must_use]
/// Creates a working directory for a test and returns the path to the created directory.
///
/// # Panics
/// Panics if creating the directory fails.
pub fn get_work_dir_for_test() -> PathBuf {
    let work_dir = std::env::temp_dir().join(format!("pex_test_{}", Uuid::new_v4()));
    std::fs::create_dir(&work_dir).unwrap();
    work_dir
}

pub(crate) trait Indent {
    fn indent(&self, spaces: usize) -> Self;
}

impl Indent for String {
    fn indent(&self, spaces: usize) -> Self {
        let mut replace = Self::from("\n");
        for _ in 0..spaces {
            replace.push(' ');
        }
        let mut result = self.replace('\n', &replace);
        result.insert_str(0, &replace);
        result
    }
}

static LOGGER: TestLogger = TestLogger::new();

#[derive(Clone)]
pub struct VirtualFile {
    buffer: Arc<Mutex<String>>,
}

impl VirtualFile {
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(String::new())),
        }
    }

    #[expect(clippy::must_use_candidate)]
    pub fn take(&self) -> String {
        let mut buffer = self.buffer.lock();
        let mut other = String::new();
        std::mem::swap(&mut *buffer, &mut other);
        other
    }
}

impl Default for VirtualFile {
    fn default() -> Self {
        Self::new()
    }
}

impl Write for VirtualFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer
            .lock()
            .push_str(std::str::from_utf8(buf).unwrap());
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum CleanUpMode {
    #[default]
    NotOnPanic,
    Always,
    #[cfg_attr(feature = "ci", expect(dead_code))]
    Never,
}

/// Provides a temporary sandbox testing environment for pexshell with tools for configuring it.
/// The environment will be cleaned up when the `TestContext` is dropped.
///
/// # Example
/// ```
/// use test_helpers::get_test_context;
///
/// let test_context = get_test_context();
/// test_context.create_config_file("~");
/// let config_path = test_context.get_config_dir().join("config.toml");
/// assert!(config_path.exists());
/// // drop test_context, which will delete the temporary test environment
/// drop(test_context);
/// assert!(!config_path.exists());
/// ```
pub struct TestContext {
    test_dir: PathBuf,
    cache_dir: PathBuf,
    config_dir: PathBuf,
    clean_up: CleanUpMode,
    stdout_buffer: Arc<Mutex<String>>,
    stderr_buffer: Arc<Mutex<String>>,
    logging_permit: Mutex<Option<TestLoggerPermit<'static>>>,
    logging_context: OnceLock<TestLoggerContext<'static>>,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        if !self.test_dir.exists() {
            return;
        }
        #[expect(
            clippy::unnecessary_debug_formatting,
            reason = "debug formatting is intentional to clearly indicate the path in logs"
        )]
        match self.clean_up {
            CleanUpMode::NotOnPanic if std::thread::panicking() => {
                warn!(
                    "Test appears to have failed due to panic - leaving behind test environment in {:?}.",
                    &self.test_dir
                );
            }
            CleanUpMode::Always | CleanUpMode::NotOnPanic => {
                info!("Cleaning up test dir...");
                std::fs::remove_dir_all(&self.test_dir).unwrap();
                info!("Done!");
            }
            CleanUpMode::Never => {
                warn!("Leaving behind test environment in {:?}.", &self.test_dir);
            }
        }
    }
}

impl TestContext {
    fn new(test_dir: PathBuf) -> Self {
        let cache_dir = test_dir.join("cache");
        let config_dir = test_dir.join("config");
        _ = log::set_logger(&LOGGER);
        log::set_max_level(LevelFilter::max());
        info!("test work dir: {}", test_dir.to_str().unwrap());
        let backtrace = std::backtrace::Backtrace::force_capture();
        std::fs::write(test_dir.join("backtrace.txt"), format!("{backtrace}")).unwrap();
        let stdout_buffer = Arc::new(Mutex::new(String::new()));
        let stderr_buffer = Arc::new(Mutex::new(String::new()));

        Self {
            test_dir,
            cache_dir,
            config_dir,
            clean_up: CleanUpMode::NotOnPanic,
            stdout_buffer,
            stderr_buffer,
            logging_permit: Mutex::new(Some(LOGGER.get_permit())),
            logging_context: OnceLock::new(),
        }
    }

    #[must_use]
    pub fn log_level(self, level: LevelFilter) -> Self {
        self.logger().get_config_mut().log_level = level;
        self
    }

    /// Prevents the test environment from being cleaned up.
    /// The environment will be cleaned up when the `TestContext` is dropped.
    ///
    /// Should only be used for debugging - it will cause CI to fail.
    /// Also note that failing tests will normally leave behind their environments anyway.
    ///
    /// # Example
    /// ```
    /// use std::path::PathBuf;
    /// use test_helpers::get_test_context;
    ///
    /// let test_context = get_test_context().no_clean_up();
    /// test_context.create_config_file(String::from("~"));
    /// let test_dir = PathBuf::from(test_context.get_test_dir());
    /// let config_path = test_context.get_config_dir().join("config.toml");
    /// assert!(config_path.exists());
    /// // drop test_context, which now will NOT delete the temporary test environment
    /// drop(test_context);
    /// assert!(config_path.exists());
    /// // std::fs::remove_dir_all(&test_dir);
    /// ```
    #[cfg(not(feature = "ci"))]
    #[must_use]
    pub fn no_clean_up(mut self) -> Self {
        self.clean_up = CleanUpMode::Never;
        self
    }

    /// Forces the test environment to be cleaned up.
    /// The environment will be cleaned up when the `TestContext` is dropped.
    ///
    /// This is ideal for when a test is expected to panic and you don't want to leave behind a mess.
    ///
    /// # Example
    /// ```
    /// # use std::panic::AssertUnwindSafe;
    /// use std::path::PathBuf;
    /// use test_helpers::get_test_context;
    ///
    /// let test_context = get_test_context().always_clean_up();
    /// test_context.create_config_file(String::from("~"));
    /// let test_dir = PathBuf::from(test_context.get_test_dir());
    /// let config_path = test_context.get_config_dir().join("config.toml");
    /// assert!(config_path.exists());
    /// // drop test_context, which now will NOT delete the temporary test environment
    /// std::panic::catch_unwind(AssertUnwindSafe(move || {
    ///    let test_context = test_context;
    ///    panic!("This should cause test_context to be dropped whilst panicking");
    /// }));
    /// assert!(!config_path.exists());
    /// ```
    #[must_use]
    pub fn always_clean_up(mut self) -> Self {
        self.clean_up = CleanUpMode::Always;
        self
    }

    /// Get the root directory of the temporary test environment.
    pub fn get_test_dir(&self) -> &Path {
        &self.test_dir
    }

    /// Get the cache directory of the temporary test environment.
    pub fn get_cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Get the config directory of the temporary test environment.
    pub fn get_config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// Create config and cache directories in the temporary test environment.
    ///
    /// # Panics
    /// Panics if creating the directories fails.
    pub fn create_test_dirs(&self) {
        std::fs::create_dir(&self.cache_dir).unwrap();
        std::fs::create_dir(&self.config_dir).unwrap();
    }

    /// Create a config file in the temporary test environment with the given string as the contents of the file.
    ///
    /// # Panics
    /// Panics if creating the config file fails.
    pub fn create_config_file(&self, contents: impl Into<String>) {
        let path = self.config_dir.join("config.toml");
        std::fs::create_dir_all(&self.config_dir).unwrap();
        std::fs::write(path, contents.into()).unwrap();
    }

    /// Used to build a pexshell config file programmatically.
    ///
    /// # Example
    /// ```
    /// use test_helpers::get_test_context;
    ///
    /// let test_context = get_test_context();
    /// test_context.get_config_builder().add_basic_user("mgr.pexip.com", "admin", "Password123", true).write();
    /// ```
    pub fn get_config_builder(&self) -> Configurer {
        Configurer::new(self)
    }

    /// Used to build a pexshell schema cache file programmatically.
    pub fn get_schema_builder(&self) -> SchemaBuilder {
        SchemaBuilder::new(self)
    }

    /// Used to build a pexshell root schema cache file programmatically.
    pub fn get_root_schema_builder(&self, api_path: impl Into<String>) -> RootSchemaBuilder {
        RootSchemaBuilder::new(self, api_path)
    }

    pub fn logger(&'_ self) -> &'_ TestLoggerContext<'_> {
        self.logging_context
            .get_or_init(|| self.logging_permit.lock().take().unwrap().promote())
    }

    pub fn get_stdout_wrapper(&self) -> impl std::io::Write {
        let buffer = Arc::clone(&self.stdout_buffer);
        VirtualFile { buffer }
    }

    pub fn get_stderr_wrapper(&self) -> impl std::io::Write {
        let buffer = Arc::clone(&self.stderr_buffer);
        VirtualFile { buffer }
    }

    /// Gets the contents of the stdout buffer, simultaneously clearing it.
    pub fn take_stdout(&self) -> String {
        let mut stdout = String::new();
        std::mem::swap(&mut stdout, &mut self.stdout_buffer.lock());
        stdout
    }

    /// Gets the contents of the stderr buffer, simultaneously clearing it.
    pub fn take_stderr(&self) -> String {
        let mut stderr = String::new();
        std::mem::swap(&mut stderr, &mut self.stderr_buffer.lock());
        stderr
    }
}

/// Gets a temporary sandbox testing environment for the current test.
#[must_use]
pub fn get_test_context() -> TestContext {
    let test_dir = get_work_dir_for_test();
    TestContext::new(test_dir)
}
