use std::path::PathBuf;
use std::{fs::File, io::Write};

use chrono::{SecondsFormat, Utc};
use log::{debug, Level, LevelFilter, Metadata, Record};
use parking_lot::Mutex;
use parking_lot::RwLock;

#[cfg(any(not(feature = "all_logs"), test))]
const PEXSHELL_MODULE_PATH: &str = "pexshell";
#[cfg(any(not(feature = "all_logs"), test))]
const PEXLIB_MODULE_PATH: &str = "pexlib";

pub struct SimpleLoggerConfig {
    log_file: Option<File>,
    log_to_stderr: bool,
}

pub struct SimpleLogger {
    config: Mutex<SimpleLoggerConfig>,
    max_level: RwLock<LevelFilter>,
}

impl SimpleLogger {
    pub fn new(log_file: Option<PathBuf>) -> std::io::Result<Self> {
        let log = match log_file {
            None => None,
            Some(path) => Some(
                std::fs::File::options()
                    .create(true)
                    .append(true)
                    .open(path)?,
            ),
        };
        Ok(Self {
            config: Mutex::new(SimpleLoggerConfig {
                log_file: log,
                log_to_stderr: false,
            }),
            max_level: RwLock::new(LevelFilter::Info),
        })
    }

    pub fn set_max_level(&self, max_level: LevelFilter) {
        *self.max_level.write() = max_level;
        debug!("Set log level to {}", max_level);
    }

    /// Sets the output file used by the logger.
    ///
    /// # Panics
    /// Will panic if writing to a log file fails.
    pub fn set_log_file(&self, log_file: Option<PathBuf>) -> std::io::Result<()> {
        {
            let mut config = self.config.lock();
            if let Some(ref mut log) = config.log_file {
                let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
                let message = log_file.as_ref().map_or_else(
                    || String::from("Logging has been switched off"),
                    |path| {
                        format!("Log file changed - subsequent logs will be written to: {path:?}")
                    },
                );
                log.write_all(format!("{timestamp} --- {message}\n",).as_bytes())
                    .expect("writing to log file failed");
            }
            config.log_file = match log_file {
                None => None,
                Some(path) => Some(
                    std::fs::File::options()
                        .create(true)
                        .append(true)
                        .open(path)?,
                ),
            };
        }
        debug!("Hello, world!");
        Ok(())
    }

    pub fn set_log_to_stderr(&self, log_to_stderr: bool) {
        self.config.lock().log_to_stderr = log_to_stderr;
    }
}

impl log::Log for SimpleLogger {
    #[cfg(not(feature = "all_logs"))]
    fn enabled(&self, metadata: &Metadata) -> bool {
        let source = metadata.target();
        let max_level =
            if source.starts_with(PEXSHELL_MODULE_PATH) || source.starts_with(PEXLIB_MODULE_PATH) {
                *self.max_level.read()
            } else {
                LevelFilter::Warn
            };

        metadata.level() <= max_level
    }

    #[cfg(feature = "all_logs")]
    fn enabled(&self, metadata: &Metadata) -> bool {
        let max_level = *self.max_level.read();
        metadata.level() <= max_level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let metadata = record.metadata();
            let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
            let level = record.level();
            let target = metadata.target();
            let args = record.args();
            let log = format!("{timestamp}  {level:<5}  {target} --- {args}");
            if let Some(ref mut f) = self.config.lock().log_file {
                // We want to explode if logging fails, because otherwise it becomes impossible to debug issues
                f.write_all(format!("{}\n", &log).as_bytes())
                    .expect("writing to log file failed");
            }
            if self.config.lock().log_to_stderr {
                let style = if level <= Level::Error {
                    console::Style::new().fg(console::Color::Red)
                } else if level <= Level::Warn {
                    console::Style::new().fg(console::Color::Yellow)
                } else {
                    console::Style::new()
                };

                eprintln!("{}", style.apply_to(log));
            }
        }
    }

    fn flush(&self) {
        if let Some(ref mut log) = self.config.lock().log_file {
            log.flush().expect("flushing log file failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Log;
    use test_case::test_case;
    use uuid::Uuid;

    #[test]
    fn test_simple_logger() {
        let log_path = std::env::temp_dir().join(format!("pexshell-test-log-{}", Uuid::new_v4()));
        let logger = SimpleLogger::new(Some(log_path.clone())).unwrap();
        logger.set_max_level(LevelFilter::Info);
        let record_1 = &Record::builder()
            .level(Level::Info)
            .args(format_args!("First test log line"))
            .target("pexshell")
            .build();
        logger.log(record_1);
        let record_2 = &Record::builder()
            .level(Level::Info)
            .args(format_args!("Second line"))
            .target("pexshell")
            .build();
        logger.log(record_2);
        logger.flush();

        let log = std::fs::read_to_string(&log_path).unwrap();
        let mut logs = log.lines();
        assert_eq!(
            logs.next().unwrap().split_once("Z  INFO ").unwrap().1,
            "  pexshell --- First test log line"
        );
        assert_eq!(
            logs.next().unwrap().split_once("Z  INFO ").unwrap().1,
            "  pexshell --- Second line"
        );
        assert!(logs.next().is_none());
        std::fs::remove_file(&log_path).unwrap();
    }

    #[test_case(LevelFilter::Error, 1)]
    #[test_case(LevelFilter::Warn, 2)]
    #[test_case(LevelFilter::Info, 3)]
    #[test_case(LevelFilter::Debug, 4)]
    #[test_case(LevelFilter::Trace, 5)]
    fn test_log_level_limit(max_level: LevelFilter, log_count: usize) {
        let log_path = std::env::temp_dir().join(format!("pexshell-test-log-{}", Uuid::new_v4()));
        let logger = SimpleLogger::new(Some(log_path.clone())).unwrap();
        logger.set_max_level(max_level);

        let record_1 = &Record::builder()
            .level(Level::Error)
            .args(format_args!("First test log line"))
            .target("pexshell")
            .build();
        logger.log(record_1);

        let record_2 = &Record::builder()
            .level(Level::Warn)
            .args(format_args!("Second line"))
            .target("pexshell")
            .build();
        logger.log(record_2);

        let record_3 = &Record::builder()
            .level(Level::Info)
            .args(format_args!("Third line"))
            .target("pexshell")
            .build();
        logger.log(record_3);

        let record_4 = &Record::builder()
            .level(Level::Debug)
            .args(format_args!("Fourth line"))
            .target("pexshell")
            .build();
        logger.log(record_4);

        let record_5 = &Record::builder()
            .level(Level::Trace)
            .args(format_args!("Fifth line"))
            .target("pexshell")
            .build();
        logger.log(record_5);

        logger.flush();

        let expected_logs = &[
            "ERROR  pexshell --- First test log line",
            "WARN   pexshell --- Second line",
            "INFO   pexshell --- Third line",
            "DEBUG  pexshell --- Fourth line",
            "TRACE  pexshell --- Fifth line",
        ][..log_count];

        let log = std::fs::read_to_string(&log_path).unwrap();
        let mut logs = log.lines();
        for &expected_log in expected_logs {
            assert_eq!(
                logs.next().unwrap().split_once("Z  ").unwrap().1,
                expected_log
            );
        }
        assert!(logs.next().is_none());
        std::fs::remove_file(&log_path).unwrap();
    }

    #[test]
    fn create_without_file_test() {
        let log_path = std::env::temp_dir().join(format!("pexshell-test-log-{}", Uuid::new_v4()));
        let logger = SimpleLogger::new(None).unwrap();
        logger.set_max_level(LevelFilter::Info);
        let record_0 = &Record::builder()
            .level(Level::Info)
            .args(format_args!("Should not be logged"))
            .target("pexshell")
            .build();
        logger.log(record_0);

        logger.set_log_file(Some(log_path.clone())).unwrap();
        let record_1 = &Record::builder()
            .level(Level::Info)
            .args(format_args!("First test log line"))
            .target("pexshell")
            .build();
        logger.log(record_1);
        let record_2 = &Record::builder()
            .level(Level::Info)
            .args(format_args!("Second line"))
            .target("pexshell")
            .build();
        logger.log(record_2);
        logger.flush();

        let log = std::fs::read_to_string(&log_path).unwrap();
        let mut logs = log.lines();
        assert_eq!(
            logs.next().unwrap().split_once("Z  INFO ").unwrap().1,
            "  pexshell --- First test log line"
        );
        assert_eq!(
            logs.next().unwrap().split_once("Z  INFO ").unwrap().1,
            "  pexshell --- Second line"
        );
        assert!(logs.next().is_none());
        std::fs::remove_file(&log_path).unwrap();
    }

    #[test]
    fn change_log_file_test() {
        let log_path_1 = std::env::temp_dir().join(format!("pexshell-test-log-{}", Uuid::new_v4()));
        let log_path_2 = std::env::temp_dir().join(format!("pexshell-test-log-{}", Uuid::new_v4()));
        let logger = SimpleLogger::new(Some(log_path_1.clone())).unwrap();
        logger.set_max_level(LevelFilter::Info);
        let record_0 = &Record::builder()
            .level(Level::Info)
            .args(format_args!("First log file"))
            .target("pexshell")
            .build();
        logger.log(record_0);

        logger.set_log_file(Some(log_path_2.clone())).unwrap();
        let record_1 = &Record::builder()
            .level(Level::Info)
            .args(format_args!("First test log line"))
            .target("pexshell")
            .build();
        logger.log(record_1);
        let record_2 = &Record::builder()
            .level(Level::Info)
            .args(format_args!("Second line"))
            .target("pexshell")
            .build();
        logger.log(record_2);
        logger.flush();

        let log = std::fs::read_to_string(&log_path_1).unwrap();
        let mut logs = log.lines();
        assert_eq!(
            logs.next().unwrap().split_once("Z  INFO ").unwrap().1,
            "  pexshell --- First log file"
        );
        assert_eq!(
            logs.next().unwrap().split_once("Z --- ").unwrap().1,
            format!("Log file changed - subsequent logs will be written to: {log_path_2:?}")
                .as_str()
        );
        assert!(logs.next().is_none());

        let log = std::fs::read_to_string(&log_path_2).unwrap();
        let mut logs = log.lines();
        assert_eq!(
            logs.next().unwrap().split_once("Z  INFO ").unwrap().1,
            "  pexshell --- First test log line"
        );
        assert_eq!(
            logs.next().unwrap().split_once("Z  INFO ").unwrap().1,
            "  pexshell --- Second line"
        );
        assert!(logs.next().is_none());
        std::fs::remove_file(&log_path_1).unwrap();
        std::fs::remove_file(&log_path_2).unwrap();
    }

    #[test_case(PEXSHELL_MODULE_PATH, Level::Trace, false)]
    #[test_case(PEXSHELL_MODULE_PATH, Level::Debug, true)]
    #[test_case(PEXSHELL_MODULE_PATH, Level::Info, true)]
    #[test_case(PEXSHELL_MODULE_PATH, Level::Warn, true)]
    #[test_case(PEXSHELL_MODULE_PATH, Level::Error, true)]
    #[test_case(PEXLIB_MODULE_PATH, Level::Trace, false)]
    #[test_case(PEXLIB_MODULE_PATH, Level::Debug, true)]
    #[test_case(PEXLIB_MODULE_PATH, Level::Info, true)]
    #[test_case(PEXLIB_MODULE_PATH, Level::Warn, true)]
    #[test_case(PEXLIB_MODULE_PATH, Level::Error, true)]
    #[test_case("other", Level::Trace, false)]
    #[test_case("other", Level::Debug, cfg!(feature = "all_logs"))]
    #[test_case("other", Level::Info, cfg!(feature = "all_logs"))]
    #[test_case("other", Level::Warn, true)]
    #[test_case("other", Level::Error, true)]
    #[test_case(&format!("{PEXSHELL_MODULE_PATH}::something"), Level::Debug, true)]
    #[test_case(&format!("{PEXLIB_MODULE_PATH}::something"), Level::Debug, true)]
    #[test_case("other::thing", Level::Debug, cfg!(feature = "all_logs"))]
    fn test_module_path_filter(module_path: &str, level: Level, should_log: bool) {
        // Arrange
        let log_path = std::env::temp_dir().join(format!("pexshell-test-log-{}", Uuid::new_v4()));
        let logger = SimpleLogger::new(Some(log_path.clone())).unwrap();
        logger.set_max_level(LevelFilter::Debug);

        let record = Record::builder()
            .level(level)
            .target(module_path)
            .args(format_args!("first record"))
            .build();

        // Act
        assert_eq!(logger.enabled(record.metadata()), should_log);
        logger.log(&record);

        // Assert
        let log = std::fs::read_to_string(log_path).unwrap();
        let log_lines: Vec<&str> = log.lines().collect();

        if should_log {
            assert_eq!(log_lines.len(), 1);
            assert!(
                log_lines[0].ends_with(&format!("Z  {level:<5}  {module_path} --- first record"))
            );
        } else {
            assert_eq!(log_lines.len(), 0);
        }
    }
}
