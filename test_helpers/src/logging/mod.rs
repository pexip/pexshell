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

pub struct TestLogger {
    config: RwLock<TestLoggerConfig>,
    expectations: Mutex<Vec<Box<dyn Expectation>>>,
}

impl TestLogger {
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: RwLock::new(TestLoggerConfig {
                log_level: log::LevelFilter::Info,
            }),
            expectations: Mutex::new(Vec::new()),
        }
    }

    #[allow(dead_code)]
    pub fn get_config(&self) -> RwLockReadGuard<TestLoggerConfig> {
        self.config.read()
    }

    pub fn get_config_mut(&self) -> RwLockWriteGuard<TestLoggerConfig> {
        self.config.write()
    }

    /// Sets an expectation.
    pub fn expect(&self, expectation: impl Expectation) {
        self.expectations.lock().push(Box::new(expectation));
    }

    /// Verifies all expectations have been met.
    ///
    /// # Panics
    /// Panics if any expectations have not been met.
    pub fn verify(&self) {
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
