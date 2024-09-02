pub mod login;

use std::collections::HashMap;
use std::io::Write;

use clap::{ArgAction, ArgMatches, Command};
use colored_json::to_colored_json_auto as to_coloured_json_auto;
use lib::mcu::schema::Methods::{Delete, Get, Patch, Post, Put};
use lib::mcu::{
    schema::{Endpoint, Field, Type},
    Api,
};
use log::{debug, warn};
use once_cell::sync::Lazy;
use serde_json::{json, Map, Value};

pub struct Console {
    is_stdout_interactive: bool,
    is_stderr_interactive: bool,
    stdout: Box<dyn Write + Send>,
    stderr: Box<dyn Write + Send>,
}

impl Console {
    pub fn new<Out: Write + Send + 'static, Error: Write + Send + 'static>(
        is_stdout_interactive: bool,
        stdout: Out,
        is_stderr_interactive: bool,
        stderr: Error,
    ) -> Self {
        Self {
            is_stdout_interactive,
            stdout: Box::new(stdout),
            is_stderr_interactive,
            stderr: Box::new(stderr),
        }
    }

    pub const fn is_stdout_interactive(&self) -> bool {
        self.is_stdout_interactive
    }

    pub const fn is_stderr_interactive(&self) -> bool {
        self.is_stderr_interactive
    }

    pub fn display_warning(&mut self, message: &str) {
        static STYLE: Lazy<console::Style> =
            Lazy::new(|| console::Style::new().fg(console::Color::Yellow));

        warn!("Displaying warning: {}", message);

        writeln!(
            self.stderr,
            "{}",
            STYLE.apply_to(format!("Warning: {message}"))
        )
        .unwrap();
    }

    pub fn pretty_print_json(&mut self, json: &Value) {
        let pretty = if self.is_stdout_interactive() {
            debug!("Stdout is a terminal - pretty-printing json in colour");
            to_coloured_json_auto(json).unwrap()
        } else {
            debug!("Stdout is not a terminal - pretty-printing json without colour");
            serde_json::to_string_pretty(json).unwrap()
        };
        writeln!(&mut self.stdout, "{pretty}").unwrap();
    }

    pub fn stderr(&mut self) -> &mut (dyn Write + Send) {
        &mut self.stderr
    }
}

impl Write for Console {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.stdout.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stdout.flush()
    }
}

pub fn generate_subcommands(schemas: &HashMap<Api, HashMap<String, Endpoint>>) -> Vec<Command> {
    let mut commands = Vec::new();
    let mut command_command = clap::Command::new("command").subcommand_required(true);

    for (api, endpoint_map) in schemas {
        match api {
            Api::Command(inner_api) => {
                command_command = command_command.subcommand(
                    clap::Command::new(inner_api.to_string().to_lowercase())
                        .subcommands(endpoint_map.iter().map(|(endpoint_name, endpoint)| {
                            generate_endpoint_subcommand_for_command_api(endpoint_name, endpoint)
                        }))
                        .subcommand_required(true),
                );
            }
            _ => {
                commands.push(
                    clap::Command::new(api.to_string().to_lowercase())
                        .subcommands(endpoint_map.iter().map(|(endpoint_name, endpoint)| {
                            generate_endpoint_subcommand(endpoint_name, endpoint)
                        }))
                        .subcommand_required(true),
                );
            }
        }
    }
    commands.push(command_command);

    commands
}

#[allow(clippy::option_if_let_else)]
fn generate_parser_for_field(
    _name: &str,
    field: &Field,
    include_types: bool,
) -> Option<clap::builder::ValueParser> {
    match field.data_type {
        Type::String => {
            if let Some(ref choices) = field.valid_choices {
                if choices.iter().all(serde_json::Value::is_string) {
                    let choices = choices
                        .iter()
                        .map(|x| {
                            String::from(x.as_str().expect("is_string implies as_str will succeed"))
                        })
                        .collect::<Vec<_>>();
                    Some(clap::builder::PossibleValuesParser::new(choices).into())
                } else {
                    None
                }
            } else {
                None
            }
        }
        Type::Boolean if include_types => Some(clap::value_parser!(bool)),
        Type::Boolean if !include_types => {
            Some(clap::builder::PossibleValuesParser::new(["true", "false"]).into())
        }
        Type::Integer if include_types => Some(clap::value_parser!(i64).into()),
        Type::Float if include_types => Some(clap::value_parser!(f64).into()),
        _ => None,
    }
}

fn generate_endpoint_subcommand(name: &str, endpoint: &Endpoint) -> clap::Command {
    let mut command = clap::Command::new(String::from(name));
    for method in &endpoint.allowed_detail_http_methods {
        command = match method {
            Get => command.subcommand(
                clap::Command::new("get")
                    .arg(
                        clap::Arg::new("object_id")
                            .action(ArgAction::Set)
                            .conflicts_with_all(["limit", "page_size", "stream"]),
                    )
                    .arg(
                        clap::Arg::new("limit")
                            .long("limit")
                            .action(ArgAction::Set)
                            .default_value("0")
                            .value_parser(clap::value_parser!(usize)),
                    )
                    .arg(
                        clap::Arg::new("page_size")
                            .long("page_size")
                            .action(ArgAction::Set)
                            .default_value("500")
                            .value_parser(clap::value_parser!(usize)),
                    )
                    .arg(
                        clap::Arg::new("stream")
                            .long("stream")
                            .action(ArgAction::SetTrue),
                    )
                    .args(endpoint.fields.iter().flat_map(|(name, field)| {
                        generate_get_field_args(
                            name,
                            field,
                            endpoint.filtering.get(name).unwrap_or(&Vec::new()),
                        )
                    })),
            ),
            Delete => command.subcommand(
                clap::Command::new("delete").arg(
                    clap::Arg::new("object_id")
                        .required(true)
                        .action(ArgAction::Set),
                ),
            ),
            #[allow(clippy::needless_collect)] // intentionally evaluate fields now
            Post => command.subcommand(
                clap::Command::new("post").args(
                    endpoint
                        .fields
                        .iter()
                        .filter_map(|(name, field)| generate_post_field_arg(name, field))
                        .collect::<Vec<clap::Arg>>(),
                ),
            ),
            #[allow(clippy::needless_collect)] // intentionally evaluate fields now
            Patch => command.subcommand(
                clap::Command::new("patch")
                    .arg(
                        clap::Arg::new("object_id")
                            .required(true)
                            .action(ArgAction::Set),
                    )
                    .args(
                        endpoint
                            .fields
                            .iter()
                            .filter_map(|(name, field)| generate_patch_field_arg(name, field))
                            .collect::<Vec<clap::Arg>>(),
                    ),
            ),
            Put => command,
        }
    }
    command.subcommand_required(true)
}

fn generate_endpoint_subcommand_for_command_api(name: &str, endpoint: &Endpoint) -> clap::Command {
    #[allow(clippy::needless_collect)] // intentionally evaluate expression now
    clap::Command::new(String::from(name)).args(
        endpoint
            .fields
            .iter()
            .filter_map(|(name, field)| generate_post_field_arg(name, field))
            .collect::<Vec<clap::Arg>>(),
    )
}

fn get_filter_args(name: &str, filters: &[String], include_self: bool) -> Vec<String> {
    let mut filters: Vec<String> = filters.iter().map(|f| format!("{name}__{f}")).collect();
    if include_self && filters.contains(&format!("{name}__exact")) {
        filters.push(String::from(name));
    }
    filters
}

const fn is_post_field(_name: &str, field: &Field) -> bool {
    !field.readonly
}

fn is_patch_field(name: &str, field: &Field) -> bool {
    !field.readonly && name != "id"
}

fn generate_post_field_arg<'a>(name: &'a str, field: &'a Field) -> Option<clap::Arg> {
    if is_post_field(name, field) {
        let mut arg = clap::Arg::new(String::from(name))
            .long(String::from(name))
            .help(String::from(&field.help_text))
            .required((!field.blank && field.default.is_none()) && !field.nullable)
            .action(ArgAction::Set);

        if let Some(value_parser) = generate_parser_for_field(name, field, true) {
            arg = arg.value_parser(value_parser);
        }
        Some(arg)
    } else {
        None
    }
}

fn generate_get_field_args<'a>(
    name: &'a str,
    field: &'a Field,
    filters: &[String],
) -> Vec<clap::Arg> {
    let mut args: Vec<clap::Arg> = get_filter_args(name, filters, false)
        .into_iter()
        .map(|filter| {
            clap::Arg::new(String::from(&filter))
                .long(filter)
                .hide_short_help(true)
                .hide_long_help(true)
                .conflicts_with("object_id")
                .action(ArgAction::Set)
        })
        .collect();

    if args
        .iter()
        .any(|a| a.get_id().as_str() == format!("{name}__exact"))
    {
        let mut help_text = field.help_text.clone();
        let options = get_filter_args(name, filters, false)
            .into_iter()
            .map(|a| format!("--{a}"))
            .reduce(|a, x| a + ", " + &x);

        if let Some(options) = options {
            help_text.push_str("\n\nFiltering options:\n");
            help_text.push_str(&options);
        }

        let mut arg = clap::Arg::new(String::from(name))
            .long(String::from(name))
            .help(String::from(&field.help_text))
            .long_help(help_text)
            .conflicts_with("object_id")
            .action(ArgAction::Set);

        if let Some(value_parser) = generate_parser_for_field(name, field, false) {
            arg = arg.value_parser(value_parser);
        }

        args.insert(0, arg);
    }

    args
}

fn generate_patch_field_arg<'a>(name: &'a str, field: &'a Field) -> Option<clap::Arg> {
    if is_patch_field(name, field) {
        let mut arg = clap::Arg::new(String::from(name))
            .long(String::from(name))
            .help(String::from(&field.help_text))
            .action(ArgAction::Set);

        if let Some(value_parser) = generate_parser_for_field(name, field, true) {
            arg = arg.value_parser(value_parser);
        }
        Some(arg)
    } else {
        None
    }
}

pub fn create_get_filters(endpoint: &Endpoint, args: &ArgMatches) -> HashMap<String, String> {
    endpoint
        .fields
        .iter()
        .flat_map(|(name, _field)| {
            get_filter_args(
                name.as_str(),
                endpoint.filtering.get(name).unwrap_or(&Vec::new()),
                true,
            )
            .into_iter()
            .filter_map(|filter| {
                args.get_one::<String>(&filter)
                    .map(|v| (filter, String::from(v)))
            })
        })
        .collect()
}

pub fn create_post_payload(endpoint: &Endpoint, args: &ArgMatches) -> Value {
    let payload: Map<String, Value> = endpoint
        .fields
        .iter()
        .filter(|(name, field)| is_post_field(name, field))
        .filter_map(|(name, field)| parse_arg_to_json(args, name, field).map(|v| (name.clone(), v)))
        .collect();

    serde_json::to_value(payload).unwrap()
}

pub fn create_patch_payload(endpoint: &Endpoint, args: &ArgMatches) -> Value {
    let payload: Map<String, Value> = endpoint
        .fields
        .iter()
        .filter(|(name, field)| is_patch_field(name, field))
        .filter_map(|(name, field)| parse_arg_to_json(args, name, field).map(|v| (name.clone(), v)))
        .collect();

    serde_json::to_value(payload).unwrap()
}

fn parse_arg_to_json(args: &ArgMatches, name: &str, field: &Field) -> Option<Value> {
    if !args.contains_id(name) {
        return None;
    }
    Some(
        if field.nullable
            && args
                .get_raw(name)
                .unwrap()
                .next()
                .unwrap()
                .to_str()
                .unwrap()
                .to_lowercase()
                == "null"
        {
            json!(null)
        } else {
            match field.data_type {
                Type::String | Type::DateTime | Type::Date | Type::Time => {
                    json!(args.get_one::<String>(name).unwrap())
                }
                Type::Related => {
                    let value = args.get_one::<String>(name).unwrap();
                    if value.trim().starts_with('{') || value.trim().starts_with('[') {
                        serde_json::from_str(value).unwrap()
                    } else {
                        json!(value)
                    }
                }
                Type::Boolean => {
                    json!(*args.get_one::<bool>(name).unwrap())
                }
                Type::Integer => {
                    json!(*args.get_one::<i64>(name).unwrap())
                }
                Type::Float => {
                    json!(*args.get_one::<f64>(name).unwrap())
                }
                Type::List => {
                    let value = args.get_one::<String>(name).unwrap();
                    serde_json::from_str(value).unwrap()
                }
                Type::File => todo!(),
            }
        },
    )
}

#[cfg(test)]
#[allow(clippy::cognitive_complexity)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use clap::error::ErrorKind::InvalidSubcommand;
    use clap::{arg, Command};
    use googletest::prelude::*;
    use lib::mcu::schema::{Endpoint, Field, Methods, Type};
    use lib::mcu::Api;
    use serde_json::json;

    use super::{create_patch_payload, create_post_payload, generate_subcommands};

    #[test]
    fn test_basic_create_post_payload() {
        // Arrange
        let endpoint = Endpoint {
            allowed_detail_http_methods: HashSet::from([Methods::Post]),
            allowed_list_http_methods: HashSet::default(),
            default_limit: 10,
            fields: HashMap::from([
                (
                    String::from("field_1"),
                    Field {
                        blank: false,
                        data_type: Type::String,
                        default: None,
                        help_text: String::new(),
                        nullable: false,
                        readonly: false,
                        related_type: None,
                        unique: false,
                        valid_choices: None,
                    },
                ),
                (
                    String::from("field_2"),
                    Field {
                        blank: false,
                        data_type: Type::String,
                        default: None,
                        help_text: String::new(),
                        nullable: false,
                        readonly: false,
                        related_type: None,
                        unique: false,
                        valid_choices: None,
                    },
                ),
            ]),
            filtering: HashMap::new(),
            ordering: Vec::new(),
        };

        let args = Command::new("Test")
            .args(&[arg!(--field_1 <field_1>), arg!(--field_2 <field_2>)])
            .try_get_matches_from(["test", "--field_1", "test 1", "--field_2", "test 2"])
            .unwrap();

        // Act
        let payload = create_post_payload(&endpoint, &args);

        // Assert
        assert_that!(
            payload,
            eq(&json!({
                "field_1": "test 1",
                "field_2": "test 2",
            }))
        );
    }

    #[test]
    fn test_basic_create_patch_payload() {
        // Arrange
        let endpoint = Endpoint {
            allowed_detail_http_methods: HashSet::from([Methods::Patch]),
            allowed_list_http_methods: HashSet::default(),
            default_limit: 10,
            fields: HashMap::from([
                (
                    String::from("field_1"),
                    Field {
                        blank: false,
                        data_type: Type::String,
                        default: None,
                        help_text: String::new(),
                        nullable: false,
                        readonly: false,
                        related_type: None,
                        unique: false,
                        valid_choices: None,
                    },
                ),
                (
                    String::from("field_2"),
                    Field {
                        blank: false,
                        data_type: Type::String,
                        default: None,
                        help_text: String::new(),
                        nullable: false,
                        readonly: false,
                        related_type: None,
                        unique: false,
                        valid_choices: None,
                    },
                ),
            ]),
            filtering: HashMap::new(),
            ordering: Vec::new(),
        };

        let args = Command::new("Test")
            .args(&[arg!(--field_1 <field_1>), arg!(--field_2 <field_2>)])
            .try_get_matches_from(["test", "--field_1", "test 1", "--field_2", "test 2"])
            .unwrap();

        // Act
        let payload = create_patch_payload(&endpoint, &args);

        // Assert
        assert_that!(
            payload,
            eq(&json!({
                "field_1": "test 1",
                "field_2": "test 2",
            }))
        );
    }

    #[test]
    fn test_allowed_methods() {
        // Arrange
        let endpoint = Endpoint {
            allowed_detail_http_methods: HashSet::from([Methods::Get, Methods::Delete]),
            allowed_list_http_methods: HashSet::default(),
            default_limit: 10,
            fields: HashMap::new(),
            filtering: HashMap::new(),
            ordering: Vec::new(),
        };
        let schemas = HashMap::from([(
            Api::Status,
            HashMap::from([(String::from("conference"), endpoint)]),
        )]);

        // Act
        let command = Command::new("Test").subcommands(generate_subcommands(&schemas));

        // Assert
        command
            .clone()
            .try_get_matches_from(["test", "status", "conference", "get"])
            .unwrap();
        command
            .clone()
            .try_get_matches_from(["test", "status", "conference", "delete", "1"])
            .unwrap();
        assert_that!(
            command
                .clone()
                .try_get_matches_from(["test", "status", "conference", "post"])
                .unwrap_err()
                .kind(),
            eq(InvalidSubcommand)
        );
        assert_that!(
            command
                .try_get_matches_from(["test", "status", "conference", "patch", "1"])
                .unwrap_err()
                .kind(),
            eq(InvalidSubcommand)
        );
    }
}
