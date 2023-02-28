#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::future_not_send)]
#![allow(clippy::missing_const_for_fn)]

pub mod error;
pub mod mcu;
pub mod util;

#[cfg(test)]
mod tests {
    use log::{Level, Log, Record};

    use crate::util::SimpleLogger;

    /// Make sure logging enabled logic is working in the lib crate
    #[test]
    fn test_logging() {
        // Arrange
        let logger = SimpleLogger::new(None).unwrap();
        logger.set_max_level(log::LevelFilter::Debug);
        let record_1 = Record::builder()
            .level(Level::Debug)
            .target(module_path!())
            .args(format_args!("first record"))
            .build();

        let record_2 = Record::builder()
            .level(Level::Trace)
            .target(module_path!())
            .args(format_args!("second record"))
            .build();

        // Act & Assert
        assert!(logger.enabled(record_1.metadata()));
        assert!(!logger.enabled(record_2.metadata()));
    }
}
