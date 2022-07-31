#![warn(
    clippy::pedantic,
    noop_method_call,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_op_in_unsafe_fn,
    unused_lifetimes,
    unused_qualifications
)]
#![allow(clippy::match_bool, clippy::needless_pass_by_value)]

fn main() -> anyhow::Result<()> {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")
        .context("failed to find manifest dir; are you running with `cargo dev`?")?;
    let mut manifest_dir = PathBuf::from(manifest_dir);
    manifest_dir.pop();
    env::set_current_dir(manifest_dir).context("failed to set current dir")?;

    match Args::parse() {
        Args::Build(args) => build(args),
        Args::Run(args) => run(args),
    }
}

/// Helper for developing rofi-bw
#[derive(Debug, clap::Parser)]
enum Args {
    Build(BuildArgs),
    Run(RunArgs),
}

/// Build rofi-bw
#[derive(Debug, clap::Parser)]
struct BuildArgs {
    /// Whether to build in the release profile (with optimizations on)
    #[clap(short, long)]
    release: bool,
}

fn build(args: BuildArgs) -> anyhow::Result<()> {
    let profile_name = match args.release {
        true => "release",
        false => "dev",
    };
    let profile_dir_name = match args.release {
        true => "release",
        false => "debug",
    };

    let status = process::Command::new("cargo")
        .arg("build")
        .arg("--package=rofi-bw-plugin")
        .arg("--package=rofi-bw")
        .args(&["--profile", profile_name])
        .status()
        .context("failed to spawn Cargo")?;

    anyhow::ensure!(status.success(), "Cargo failed");

    let target_base = PathBuf::from_iter(["build", "target", profile_dir_name]);

    let build_dir = Path::new("build");
    let lib_dir = build_dir.join("lib");
    fs::create_dir_all(&*lib_dir).context("failed to create lib dir")?;
    fs::rename(
        target_base.join("librofi_bw_plugin.so"),
        lib_dir.join("plugin.so"),
    )
    .context("failed to move plugin file")?;

    fs::rename(target_base.join("rofi-bw"), build_dir.join("rofi-bw"))
        .context("failed to move executable")?;

    Ok(())
}

/// Build and run rofi-bw, without installing it
#[derive(Debug, clap::Parser)]
struct RunArgs {
    #[clap(flatten)]
    build_args: BuildArgs,

    #[clap(last(true))]
    rest: Vec<OsString>,
}

fn run(args: RunArgs) -> anyhow::Result<()> {
    build(args.build_args)?;

    let status = process::Command::new(PathBuf::from_iter(["build", "rofi-bw"]))
        .env("ROFI_BW_LIB_DIR", PathBuf::from_iter(["build", "lib"]))
        // Put a path that will always fail at start for extra testing
        .env("ROFI_BW_RESOURCES_DIR", "doesnt-exist:resources")
        .args(args.rest)
        .status()
        .context("failed to spawn rofi-bw")?;

    anyhow::ensure!(status.success(), "rofi-bw failed");

    Ok(())
}

use anyhow::Context as _;
use clap::Parser as _;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process;
