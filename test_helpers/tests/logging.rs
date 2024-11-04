use log::info;
use test_helpers::{
    all, get_test_context,
    logging::expect::{contains, level},
};

#[test]
fn test_expect_log() {
    let test_context = get_test_context();
    let test_logger = test_context.logger();
    test_logger.expect(all!(level(log::Level::Info), contains("Test info log")));
    info!("Test info log");
    test_logger.verify();
}

#[test]
#[should_panic]
fn test_expect_log_fails() {
    let test_context = get_test_context().always_clean_up();
    let test_logger = test_context.logger();
    test_logger.expect(all!(level(log::Level::Info), contains("Test info log")));
    test_logger.verify();
}
