use serde_json::Value;
use test_helpers::{fs::Configurer, TestContext};

pub fn configure_config_test_user(
    test_context: &TestContext,
    address: impl Into<String>,
) -> Configurer {
    let configurer = test_context.get_config_builder().add_basic_user(
        address,
        "test_user",
        "test_password",
        true,
    );
    configurer.write();
    configurer
}

pub fn configure_schemas_configuration_conference_only(test_context: &TestContext) {
    test_context
        .get_schema_builder()
        .field("id", |f| {
            f.blank(true)
                .nullable(false)
                .unique(true)
                .default(Value::String(String::new()))
        })
        .field("name", |f| f.unique(true).nullable(false))
        .write("configuration/conference.json");
    test_context
        .get_root_schema_builder("/api/admin/configuration/v1/")
        .entry("conference")
        .write("configuration/root.json");
    test_context
        .get_root_schema_builder("/api/admin/status/v1/")
        .write("status/root.json");
    test_context
        .get_root_schema_builder("/api/admin/history/v1/")
        .write("history/root.json");
    test_context
        .get_root_schema_builder("/api/admin/command/v1/conference/")
        .write("command/conference/root.json");
    test_context
        .get_root_schema_builder("/api/admin/command/v1/participant/")
        .write("command/participant/root.json");
    test_context
        .get_root_schema_builder("/api/admin/command/v1/platform/")
        .write("command/platform/root.json");
}

pub fn configure_schemas_command_conference_lock_only(test_context: &TestContext) {
    test_context
        .get_schema_builder()
        .field("conference_id", |f| {
            f.blank(false)
                .nullable(false)
                .readonly(false)
                .field_type("string")
                .unique(false)
        })
        .write("command/conference/lock.json");
    test_context
        .get_root_schema_builder("/api/admin/configuration/v1/")
        .write("configuration/root.json");
    test_context
        .get_root_schema_builder("/api/admin/status/v1/")
        .write("status/root.json");
    test_context
        .get_root_schema_builder("/api/admin/history/v1/")
        .write("history/root.json");
    test_context
        .get_root_schema_builder("/api/admin/command/v1/conference/")
        .entry("lock")
        .write("command/conference/root.json");
    test_context
        .get_root_schema_builder("/api/admin/command/v1/participant/")
        .write("command/participant/root.json");
    test_context
        .get_root_schema_builder("/api/admin/command/v1/platform/")
        .write("command/platform/root.json");
}
