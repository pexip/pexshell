#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
//#![warn(clippy::cargo)]
#![allow(clippy::wildcard_imports)]
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
    sync::Arc,
};

use base64::engine::general_purpose::STANDARD as base64;
use base64::Engine;
use fs::{Configurer, RootSchemaBuilder, SchemaBuilder};
use log::{info, warn, LevelFilter};
use logging::{TestLogger, TestLoggerContext, TestLoggerPermit};
use once_cell::sync::OnceCell;
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

    #[allow(clippy::must_use_candidate)]
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
    clean_up: bool,
    stdout_buffer: Arc<Mutex<String>>,
    stderr_buffer: Arc<Mutex<String>>,
    logging_permit: Mutex<Option<TestLoggerPermit<'static>>>,
    logging_context: OnceCell<TestLoggerContext<'static>>,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        if std::thread::panicking() {
            warn!(
                "Test appears to have failed due to panic - leaving behind test environment in \
                 {:?}.",
                &self.test_dir
            );
        } else {
            self.logger().verify();
            if self.clean_up && self.test_dir.exists() {
                info!("Cleaning up test dir...");
                std::fs::remove_dir_all(&self.test_dir).unwrap();
            }
            info!("Done!");
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
        let stdout_buffer = Arc::new(Mutex::new(String::new()));
        let stderr_buffer = Arc::new(Mutex::new(String::new()));

        Self {
            test_dir,
            cache_dir,
            config_dir,
            clean_up: true,
            stdout_buffer,
            stderr_buffer,
            logging_permit: Mutex::new(Some(LOGGER.get_permit())),
            logging_context: OnceCell::new(),
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
    /// Also note that failing tests **always** leave behind their environments.
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
    /// std::fs::remove_dir_all(&test_dir);
    /// ```
    #[cfg(not(feature = "ci"))]
    #[must_use]
    pub fn no_clean_up(mut self) -> Self {
        self.clean_up = false;
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
    /// test_context.get_config_builder().add_user("mgr.pexip.com", "admin", "Password123", true).write();
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

    #[allow(clippy::unused_self)]
    pub fn logger(&self) -> &TestLoggerContext {
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
}

/// Gets a temporary sandbox testing environment for the current test.
#[must_use]
pub fn get_test_context() -> TestContext {
    let test_dir = get_work_dir_for_test();
    TestContext::new(test_dir)
}

/// A matcher for use with `httptest::Server`. Matches on basic auth credentials.
pub struct BasicAuthMatcher {
    credential: String,
}

impl<B> httptest::matchers::Matcher<http::Request<B>> for BasicAuthMatcher {
    fn matches(
        &mut self,
        input: &http::Request<B>,
        _ctx: &mut httptest::matchers::ExecutionContext,
    ) -> bool {
        let auth_headers = input
            .headers()
            .get_all("authorization")
            .into_iter()
            .collect::<Vec<_>>();

        if auth_headers.len() == 1 {
            let auth_header = auth_headers[0];
            if let Some(value) = auth_header
                .to_str()
                .ok()
                .and_then(|x| x.strip_prefix("Basic "))
            {
                if let Ok(Ok(value)) = base64.decode(value).map(String::from_utf8) {
                    return value == self.credential;
                }
            }
        }
        false
    }

    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_tuple("BasicAuthMatcher")
            .field(&self.credential)
            .finish()
    }
}

/// Matches on basic auth credentials. For use with `httptest::Server`.
///
/// # Example
/// ```
/// use httptest::{Server, Expectation, responders::status_code};
/// use test_helpers::has_credentials;
/// use reqwest;
///
/// let server = Server::run();
/// server.expect(Expectation::matching(has_credentials("some_user", "some_password")).respond_with(status_code(200)));
/// let client = reqwest::blocking::Client::new();
/// let response = client.get(server.url_str("")).basic_auth("some_user", Some("some_password")).send().unwrap();
/// assert_eq!(response.status(), 200)
/// ```
#[must_use]
pub fn has_credentials(username: &str, password: &str) -> BasicAuthMatcher {
    BasicAuthMatcher {
        credential: format!("{username}:{password}"),
    }
}
