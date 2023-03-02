#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
//#![warn(clippy::cargo)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_const_for_fn)]

mod argparse;
mod cli;
mod config;
mod consts;
#[cfg(test)]
mod end_to_end_tests;
mod pexshell;
#[cfg(test)]
mod test_util;

use clap::ArgMatches;
use cli::Console;
use git_version::git_version;
use is_terminal::IsTerminal;
use lazy_static::lazy_static;
use lib::{
    error,
    mcu::{self, schema, Api},
    util::SimpleLogger,
};
use log::{error, warn, LevelFilter};
use parking_lot::RwLock;
use serde_json::Value;
#[cfg(unix)]
use simple_signal::Signal;
use std::{collections::HashMap, path::PathBuf};
use tokio::io::AsyncReadExt;

#[cfg(unix)]
use crate::consts::EXIT_CODE_INTERRUPTED;

static ABORT_ON_INTERRUPT: RwLock<bool> = RwLock::new(true);

lazy_static! {
    static ref LOGGER: SimpleLogger =
        SimpleLogger::new(None).expect("creating a logger without a file should not fail");
    static ref VERSION: String = pexshell_version();
}

/// Equivalent of `git describe --dirty=-modified | sed 's/-g/-/'`
fn pexshell_version() -> String {
    let git_version: &str = git_version!(args = ["--dirty=-dirty", "--tags"]);
    git_version
        .replace("-g", "-")
        .strip_prefix('v')
        .unwrap()
        .to_owned()
}

#[allow(clippy::too_many_lines)]
fn api_request_from_matches(
    matches: &ArgMatches,
    schemas: &HashMap<Api, HashMap<String, schema::Endpoint>>,
) -> Result<(mcu::ApiRequest, bool), error::UserFriendly> {
    let (api, sub_m) = match matches.subcommand() {
        Some(("configuration", sub_m)) => Ok((mcu::Api::Configuration, sub_m)),
        Some(("status", sub_m)) => Ok((mcu::Api::Status, sub_m)),
        Some(("command", sub_c)) => match sub_c.subcommand() {
            Some(("participant", sub_m)) => {
                Ok((mcu::Api::Command(mcu::CommandApi::Participant), sub_m))
            }
            Some(("conference", sub_m)) => {
                Ok((mcu::Api::Command(mcu::CommandApi::Conference), sub_m))
            }
            Some(("platform", sub_m)) => Ok((mcu::Api::Command(mcu::CommandApi::Platform), sub_m)),
            o => Err(error::UserFriendly::new(
                format!("Unrecognised API {o:?}!",),
            )),
        },
        Some(("history", sub_m)) => Ok((mcu::Api::History, sub_m)),
        o => Err(error::UserFriendly::new(format!("unrecognised API {o:?}!"))),
    }?;

    let endpoint_map = schemas
        .get(&api)
        .ok_or_else(|| error::UserFriendly::new(format!("unrecognised api {api}")))?;

    let (resource, sub_m) = sub_m
        .subcommand()
        .ok_or_else(|| error::UserFriendly::new("unrecognised path!"))?;

    let endpoint = endpoint_map
        .get(resource)
        .ok_or_else(|| error::UserFriendly::new(format!("unrecognised resource {resource}")))?;

    let api_request = if let Api::Command(_) = &api {
        let payload = cli::create_post_payload(endpoint, sub_m);
        Ok((
            mcu::ApiRequest::Post {
                api,
                resource: resource.to_string(),
                args: payload,
            },
            false,
        ))
    } else {
        match sub_m.subcommand() {
            Some(("get", sub_m)) => sub_m.get_one::<String>("object_id").map_or_else(
                || {
                    let page_size = *sub_m
                        .get_one::<usize>("page_size")
                        .expect("clap should validate page_size");
                    let limit = *sub_m
                        .get_one::<usize>("limit")
                        .expect("clap should validate limit");
                    let stream = sub_m.get_flag("stream");
                    Ok((
                        mcu::ApiRequest::GetAll {
                            api,
                            resource: resource.to_string(),
                            filter_args: cli::create_get_filters(endpoint, sub_m),
                            page_size,
                            limit,
                            offset: 0,
                        },
                        stream,
                    ))
                },
                |id| {
                    Ok((
                        mcu::ApiRequest::Get {
                            api,
                            resource: String::from(resource),
                            object_id: String::from(id),
                        },
                        false,
                    ))
                },
            ),
            Some(("post", sub_m)) => {
                let payload = cli::create_post_payload(endpoint, sub_m);
                Ok((
                    mcu::ApiRequest::Post {
                        api,
                        resource: resource.to_string(),
                        args: payload,
                    },
                    false,
                ))
            }
            Some(("patch", sub_m)) => {
                let payload = cli::create_patch_payload(endpoint, sub_m);
                Ok((
                    mcu::ApiRequest::Patch {
                        api,
                        resource: resource.to_string(),
                        object_id: String::from(
                            sub_m
                                .get_one::<String>("object_id")
                                .expect("clap should validate object_id"),
                        ),
                        args: payload,
                    },
                    false,
                ))
            }
            Some(("delete", sub_m)) => Ok((
                mcu::ApiRequest::Delete {
                    api,
                    resource: resource.to_string(),
                    object_id: String::from(
                        sub_m
                            .get_one::<String>("object_id")
                            .expect("clap should validate object_id"),
                    ),
                },
                false,
            )),
            _ => Err(error::UserFriendly::new("Unrecognised mode!")),
        }
    }?;

    Ok(api_request)
}

#[allow(dead_code)]
async fn read_stdin_to_json() -> anyhow::Result<Option<Value>> {
    let mut contents = String::new();
    let _bytes_read = tokio::io::stdin().read_to_string(&mut contents).await?;
    if contents.is_empty() {
        Ok(None)
    } else {
        let contents = serde_json::from_str(&contents)?;
        Ok(Some(contents))
    }
}

fn set_abort_on_interrupt(abort_on_interrupt: bool) {
    *ABORT_ON_INTERRUPT.write() = abort_on_interrupt;
}

pub struct Directories {
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub tmp_dir: PathBuf,
}

impl Default for Directories {
    fn default() -> Self {
        let base_dirs = directories::BaseDirs::new().expect("could not find user base directories");
        Self {
            config_dir: base_dirs.config_dir().join("pexip/pexshell"),
            cache_dir: base_dirs.cache_dir().join("pexip/pexshell"),
            tmp_dir: std::env::temp_dir().join("pexip/pexshell"),
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    lazy_static! {
        static ref ERROR_STYLE: console::Style = console::Style::new().fg(console::Color::Red);
        static ref PLAIN_STYLE: console::Style = console::Style::new();
    }
    log::set_max_level(LevelFilter::max());
    log::set_logger(&*LOGGER).expect("this can only fail if a logger has already been set");

    #[cfg(unix)]
    simple_signal::set_handler(&[Signal::Int], |signals| {
        if *ABORT_ON_INTERRUPT.read() {
            error!("received signals: {signals:?} - aborting");
            std::process::exit(EXIT_CODE_INTERRUPTED);
        } else {
            error!("received signals: {signals:?}");
        }
    });

    let args: Vec<String> = std::env::args().collect();
    let is_stderr_interactive = std::io::stderr().is_terminal();
    let dirs = Directories::default();

    let stdout = std::io::stdout();
    let is_stdout_interactive = stdout.is_terminal();
    let console = Console::new(is_stdout_interactive, stdout);

    let env: HashMap<String, String> = std::env::vars().collect();

    let mut pexshell = pexshell::PexShell::new(&dirs, console, env);
    let result = pexshell.run(args).await;

    if let Err(e) = result {
        error!("fatal error occurred: {e:?}");

        let style = if is_stderr_interactive {
            &*ERROR_STYLE
        } else {
            &*PLAIN_STYLE
        };
        eprintln!("{}", style.apply_to(e.to_string()));
    }
}

#[cfg(test)]
#[allow(clippy::implicit_hasher)]
pub async fn run_with(
    args: &[String],
    env: HashMap<String, String>,
    dirs: &Directories,
    stdout_wrapper: impl std::io::Write + Send + 'static,
) -> anyhow::Result<()> {
    let mut pexshell = pexshell::PexShell::new(dirs, Console::new(false, stdout_wrapper), env);
    pexshell.run(args.to_vec()).await
}
