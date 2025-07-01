#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_const_for_fn)]

pub mod error;
pub mod mcu;
pub mod util;

#[cfg(any(test, feature = "test_util"))]
pub mod test_util;

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
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
        assert_that!(logger.enabled(record_1.metadata()), eq(true));
        assert_that!(logger.enabled(record_2.metadata()), eq(false));
    }
}
