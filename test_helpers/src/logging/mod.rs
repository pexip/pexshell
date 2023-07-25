use std::io::Write;

use chrono::{SecondsFormat, Utc};
use console::Color;
use expect::Expectation;
use log::Level;
use parking_lot::{Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub mod expect;

use crate::{logging::expect::MatchResult, Indent};

pub struct TestLoggerConfig {
    pub log_level: log::LevelFilter,
}

impl Default for TestLoggerConfig {
    fn default() -> Self {
        Self {
            log_level: log::LevelFilter::Info,
        }
    }
}

pub struct TestLoggerPermit<'a> {
    test_logger: &'a TestLogger,
    guard: RwLockReadGuard<'a, ()>,
}

impl<'a> TestLoggerPermit<'a> {
    /// Promote the permit to a [`TestLoggerContext`], providing exclusive access to the [`TestLogger`] and allowing
    /// expectations to be set.
    ///
    /// Note that to prevent deadlock, this function will briefly unlock the test lock, potentially allowing another
    /// thread to be promoted first.
    #[must_use]
    pub fn promote(self) -> TestLoggerContext<'a> {
        drop(self.guard);

        self.test_logger.get_context()
    }
}

pub struct TestLoggerContext<'a> {
    test_logger: &'a TestLogger,
    _guard: RwLockWriteGuard<'a, ()>,
}

impl<'a> TestLoggerContext<'a> {
    #[allow(dead_code)]
    pub fn get_config(&self) -> RwLockReadGuard<TestLoggerConfig> {
        self.test_logger.get_config()
    }

    pub fn get_config_mut(&self) -> RwLockWriteGuard<TestLoggerConfig> {
        self.test_logger.get_config_mut()
    }

    /// Set an expectation.
    pub fn expect(&self, expectation: impl Expectation) {
        self.test_logger.expect(expectation);
    }

    /// Verify all expectations have been met.
    ///
    /// # Panics
    /// Panics if any expectations have not been met.
    pub fn verify(&self) {
        self.test_logger.verify();
    }

    /// Check if expectations have been met without panicking if they haven't.
    #[must_use]
    pub fn expectations_met(&self) -> bool {
        self.test_logger.expectations_met()
    }

    /// Clear all unmet expectations without verifying them.
    pub fn clear(&self) {
        self.test_logger.clear();
    }
}

impl<'a> Drop for TestLoggerContext<'a> {
    fn drop(&mut self) {
        self.clear();
    }
}

pub struct TestLogger {
    config: RwLock<TestLoggerConfig>,
    expectations: Mutex<Vec<Box<dyn Expectation>>>,
    // Required due to logging being global
    test_lock: RwLock<()>,
}

impl TestLogger {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            config: RwLock::new(TestLoggerConfig {
                log_level: log::LevelFilter::Info,
            }),
            expectations: Mutex::new(Vec::new()),
            test_lock: RwLock::new(()),
        }
    }

    /// Grants exclusive access to the [`TestLogger`] and provides an interface to set expectations.
    ///
    /// Note that calling this function will cause a deadlock if the thread already holds a [`TestLoggerPermit`]!
    /// In this case, you should instead call `TestLoggerPermit::promote`.
    pub fn get_context(&self) -> TestLoggerContext {
        TestLoggerContext {
            test_logger: self,
            _guard: self.test_lock.write(),
        }
    }

    /// Grants permission to log to this [`TestLogger`].
    /// Can be promoted to a [`TestLoggerContext`] to provide exclusive access to the [`TestLogger`]
    /// for setting expectations.
    pub fn get_permit(&self) -> TestLoggerPermit {
        TestLoggerPermit {
            test_logger: self,
            guard: self.test_lock.read(),
        }
    }

    #[allow(dead_code)]
    fn get_config(&self) -> RwLockReadGuard<TestLoggerConfig> {
        self.config.read()
    }

    fn get_config_mut(&self) -> RwLockWriteGuard<TestLoggerConfig> {
        self.config.write()
    }

    fn expect(&self, expectation: impl Expectation) {
        self.expectations.lock().push(Box::new(expectation));
    }

    fn verify(&self) {
        let expectations = self.expectations.lock();
        if expectations.is_empty() {
            return;
        }
        let mut expectation_message_list = String::new();
        for expectation in &*expectations {
            expectation_message_list += &format!("{expectation:?}").indent(4);
        }
        panic!("Some logging expectations were not met:\n{expectation_message_list}");
    }

    #[must_use]
    fn expectations_met(&self) -> bool {
        self.expectations.lock().is_empty()
    }

    fn clear(&self) {
        self.expectations.lock().clear();
    }
}

impl Default for TestLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl log::Log for TestLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.config.read().log_level
    }

    fn log(&self, record: &log::Record) {
        let metadata = record.metadata();
        let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let level = record.level();
        let log = format!(
            "{}  {}  {} --- {}",
            timestamp,
            level,
            metadata.target(),
            record.args()
        );

        let style = if level <= Level::Error {
            console::Style::new().fg(Color::Red).bold()
        } else if level <= Level::Warn {
            console::Style::new().fg(Color::Yellow).bold()
        } else {
            console::Style::new()
        };
        eprintln!("{}", style.apply_to(log));

        let mut expectations = self.expectations.lock();
        let mut maybe_remove = None;
        for (i, expectation) in expectations.iter_mut().enumerate() {
            match expectation.matches(record) {
                MatchResult::NotMatch => {}
                MatchResult::Match => break,
                MatchResult::Complete => {
                    maybe_remove = Some(i);
                    break;
                }
            }
        }

        if let Some(remove) = maybe_remove {
            expectations.remove(remove);
        }
    }

    fn flush(&self) {
        std::io::stderr().flush().unwrap();
    }
}
