use std::collections::HashMap;

use httptest::{
    matchers::{all_of, request},
    responders::json_encoded,
    Expectation, Server,
};
use serde_json::json;
use test_helpers::{get_test_context, has_credentials};

use crate::test_util::TestContextExtensions;

mod configuration_helpers;

mod cache;
mod delete;
mod get;
mod get_all;
mod patch;
mod post;

#[test]
fn basic_get() {
    let test_context = get_test_context();
    let server = Server::run();

    server.expect(
        Expectation::matching(all_of![
            request::method_path("GET", "/api/admin/configuration/v1/conference/1/"),
            has_credentials("test_user", "test_password"),
        ])
        .respond_with(json_encoded(json_response())),
    );

    let config = format!(
        r#"
        [[users]]
        address = "http://{}"
        username = "test_user"
        password = "test_password"
        current_user = true
        "#,
        server.addr(),
    );

    test_context.create_config_file(config);
    std::fs::create_dir_all(test_context.get_cache_dir().join("schemas/configuration")).unwrap();
    std::fs::write(
        test_context
            .get_cache_dir()
            .join("schemas/configuration/root.json"),
        serde_json::to_string(&json!({
            "conference": {
                "list_endpoint": "/api/admin/configuration/v1/conference/",
                "schema": "/api/admin/configuration/v1/conference/schema/"
            }
        }))
        .unwrap(),
    )
    .unwrap();
    for dir in [
        "status",
        "history",
        "command/conference",
        "command/participant",
        "command/platform",
    ] {
        std::fs::create_dir_all(test_context.get_cache_dir().join("schemas").join(dir)).unwrap();
        std::fs::write(
            test_context
                .get_cache_dir()
                .join("schemas")
                .join(dir)
                .join("root.json"),
            "{}",
        )
        .unwrap();
    }

    std::fs::write(
        test_context
            .get_cache_dir()
            .join("schemas/configuration/conference.json"),
        serde_json::to_string(&json_schema()).unwrap(),
    )
    .unwrap();

    test_context
        .block_on(crate::run_with(
            &["pexshell", "configuration", "conference", "get", "1"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>(),
            HashMap::default(),
            &test_context.get_directories(),
            test_context.get_stdout_wrapper(),
            test_context.get_stderr_wrapper(),
        ))
        .unwrap();
}

fn json_response() -> serde_json::Value {
    json!({
        "aliases": [],
        "creation_time": "2022-04-01T15:58:06.714308",
        "crypto_mode": "",
        "description": "",
        "guest_pin": "",
        "guests_can_present": true,
        "id": 1,
        "ivr_theme": null,
        "max_pixels_per_second": "fullhd",
        "media_playlist": null,
        "mute_all_guests": false,
        "participant_limit": 30,
        "pin": "",
        "resource_uri": "/api/admin/configuration/v1/conference/1/"
    })
}

#[allow(clippy::too_many_lines)]
fn json_schema() -> serde_json::Value {
    json!({
        "allowed_detail_http_methods": [
            "get",
            "post",
            "put",
            "delete",
            "patch"
        ],
        "allowed_list_http_methods": [
            "get",
            "post",
            "put",
            "delete",
            "patch"
        ],
        "default_format": "application/json",
        "default_limit": 20,
        "fields": {
            "aliases": {
                "blank": false,
                "default": null,
                "help_text": "The aliases associated with this conference.",
                "nullable": true,
                "readonly": false,
                "related_type": "to_many",
                "type": "related",
                "unique": false
            },
            "creation_time": {
                "blank": false,
                "default": "2022-04-01T15:58:06.714308",
                "help_text": "The time at which the configuration was created.",
                "nullable": false,
                "readonly": false,
                "type": "datetime",
                "unique": false
            },
            "crypto_mode": {
                "blank": false,
                "default": "",
                "help_text": "Controls the media encryption requirements for participants connecting to this service. Use global setting: Use the global media encryption setting (Platform > Global Settings). Required: All participants (including RTMP participants) must use media encryption. Best effort: Each participant will use media encryption if their device supports it, otherwise the connection will be unencrypted. No encryption: All H.323, SIP and MS-SIP participants must use unencrypted media. (RTMP participants will use encryption if their device supports it, otherwise the connection will be unencrypted.)",
                "nullable": true,
                "readonly": false,
                "type": "string",
                "unique": false,
                "valid_choices": [
                    "besteffort",
                    "on",
                    "off"
                ]
            },
            "description": {
                "blank": true,
                "default": "",
                "help_text": "A description of the service. Maximum length: 250 characters.",
                "nullable": false,
                "readonly": false,
                "type": "string",
                "unique": false
            },
            "guest_pin": {
                "blank": true,
                "default": "",
                "help_text": "This optional field allows you to set a secure access code for Guest participants who dial in to the service. Length: 4-20 digits, including any terminal #.",
                "nullable": false,
                "readonly": false,
                "type": "string",
                "unique": false
            },
            "guests_can_present": {
                "blank": false,
                "default": true,
                "help_text": "If enabled, Guests and Hosts can present into the conference. If disabled, only Hosts can present.",
                "nullable": false,
                "readonly": false,
                "type": "boolean",
                "unique": false,
                "valid_choices": [
                    true,
                    false
                ]
            },
            "id": {
                "blank": true,
                "default": "",
                "help_text": "The primary key.",
                "nullable": false,
                "readonly": false,
                "type": "integer",
                "unique": true
            },
            "ivr_theme": {
                "blank": false,
                "default": null,
                "help_text": "The theme for use with this service.",
                "nullable": true,
                "readonly": false,
                "related_type": "to_one",
                "type": "related",
                "unique": false
            },
            "max_pixels_per_second": {
                "blank": false,
                "default": null,
                "help_text": "Sets the maximum call quality for each participant.",
                "nullable": true,
                "readonly": false,
                "type": "string",
                "unique": false,
                "valid_choices": [
                    "sd",
                    "hd",
                    "fullhd"
                ]
            },
            "media_playlist": {
                "blank": false,
                "default": null,
                "help_text": "The playlist to run when this Media Playback Service is used.",
                "nullable": true,
                "readonly": false,
                "related_type": "to_one",
                "type": "related",
                "unique": false
            },
            "mute_all_guests": {
                "blank": false,
                "default": false,
                "help_text": "If enabled, all Guest participants will be muted by default.",
                "nullable": false,
                "readonly": false,
                "type": "boolean",
                "unique": false,
                "valid_choices": [
                    true,
                    false
                ]
            },
            "participant_limit": {
                "blank": false,
                "default": null,
                "help_text": "This optional field allows you to limit the number of participants allowed to join this Virtual Meeting Room. Range: 0 to 1000000.",
                "nullable": true,
                "readonly": false,
                "type": "integer",
                "unique": false
            },
            "pin": {
                "blank": true,
                "default": "",
                "help_text": "This optional field allows you to set a secure access code for participants who dial in to the service. Length: 4-20 digits, including any terminal #.",
                "nullable": false,
                "readonly": false,
                "type": "string",
                "unique": false
            },
            "resource_uri": {
                "blank": false,
                "help_text": "The URI that identifies this resource.",
                "nullable": false,
                "readonly": true,
                "type": "string",
                "unique": false
            },
        },
        "filtering": {
            "aliases": 2,
            "allow_guests": 1,
            "creation_time": 1,
            "description": 1,
            "enable_chat": 1,
            "enable_overlay_text": 1,
            "gms_access_token": 2,
            "guest_pin": 1,
            "ivr_theme": 2,
            "max_callrate_in": 1,
            "max_callrate_out": 1,
            "media_playlist": 2,
            "mssip_proxy": 2,
            "name": 1,
            "participant_limit": 1,
            "pin": 1,
            "primary_owner_email_address": 1,
            "scheduled_conferences": 2,
            "scheduled_conferences_count": 1,
            "service_type": 1,
            "system_location": 2,
            "tag": 1,
            "two_stage_dial_type": 1
        },
        "ordering": [
            "name",
            "tag",
            "description",
            "service_type",
            "pin",
            "allow_guests",
            "enable_chat",
            "guest_pin",
            "max_callrate_in",
            "max_callrate_out",
            "participant_limit",
            "primary_owner_email_address",
            "enable_overlay_text",
            "creation_time"
        ]
    })
}
