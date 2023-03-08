# Pexshell

![build status](https://github.com/pexip/pexshell/actions/workflows/ci.yml/badge.svg?branch=master)

Pexshell is a CLI tool for interacting with the Pexip Infinity Management API. For more information regarding the API see the [Pexip Docs](https://docs.pexip.com/admin/integrate_api.htm).

## Installation (from source)

1. Install [git](https://git-scm.com/) and the rust toolchain ([start here](https://www.rust-lang.org/learn/get-started))
2. Run `git clone https://github.com/pexip/pexshell && cargo install --path pexshell`. Cargo will then build and install the Pexshell binary. Note down the directory it installs to and ensure it is in your `PATH`.

## Usage

Use `pexshell --help` for information on what commands you can use.
On first use, you should run `pexshell login` and input your login details, followed by `pexshell cache` to generate the schema cache.
Following this, you should see new subcommands appear in the output of `pexshell --help` (`configuration`, `status`, etc.).

> **Note:** if you're getting certificate errors, you can try using the `--insecure` switch (e.g. `pexshell --insecure login`) to switch off certificate verification, however bear in mind this has severe security implications and therefore should only be used inside a secure and trusted network environment.
> A better solution is to install the appropriate certificate to your operating system's certificate store.

We can see what commands are available, for instance on the configuration API, by running `pexshell configuration --help`.
This gives us a list of the subcommands that represent API endpoints on the `configuration` API.
We can then use `pexshell configuration conference --help` to see what options we have for the conference endpoint.
Doing a `get` on the endpoint (`pexshell configuration conference get`) will return all of the objects for that resource.

We can also get by ID (`pexshell configuration conference get <id>`) or even use filters.
`pexshell configuration conference get --help` will list the filters we can use, e.g. `pexshell configuration conference get --name__startswith a` will get all the conference objects whose `name` field begins with the letter `a`.

Since the API returns JSON, it's useful to pair Pexshell with [jq](https://stedolan.github.io/jq/) -- a command-line JSON processor.
As a simple example, we could list the names of all conferences that start with `a` with the following command:

```sh
pexshell configuration conference get --name__startswith a | jq -r '.[].name'
```

You can find more usage examples in [EXAMPLES.md](https://github.com/pexip/pexshell/blob/master/EXAMPLES.md).

### Unattended/simultaneous login

To facilitate use of Pexshell in scripts, you can override login details by setting the `PEXSHELL_ADDRESS`, `PEXSHELL_USERNAME` and `PEXSHELL_PASSWORD` environment variables (to the management node address, username and password respectively).
If the user's credentials are already stored (they have logged in using the interactive `pexshell login` command) then the `PEXSHELL_PASSWORD` variable can be omitted and it will be retrieved from the credential store.

## Logging

Logging can be used if required for further debugging. The log level can be set in the config file under the log section:

```toml
[log]
file = "/path/to/logfile.log"
level = "debug"
stderr = true
```

The values in the config file can be overridden with the environment variables `PEXSHELL_LOG_LEVEL`, `PEXSHELL_LOG_FILE` and `PEXSHELL_LOG_TO_STDERR`. If `stderr` or `PEXSHELL_LOG_TO_STDERR` is set then logs will also be output to `STDERR` as well as the configured log file.

## Licenses

A full list of third-party dependencies and their licenses can be generated with `cargo-about`.

Install with:

```sh
cargo install cargo-about
```

then generate the full list of third party licenses with:

```sh
cargo about generate about.hbs > licenses.html
```
