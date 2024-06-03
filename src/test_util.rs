use test_helpers::TestContext;

use crate::Directories;

pub use lib::test_util::*;

pub trait TestContextExtensions {
    fn get_directories(&self) -> Directories;
}

impl TestContextExtensions for TestContext {
    fn get_directories(&self) -> Directories {
        Directories {
            config_dir: self.get_config_dir().to_path_buf(),
            cache_dir: self.get_cache_dir().to_path_buf(),
            tmp_dir: self.get_test_dir().join("tmp"),
        }
    }
}
