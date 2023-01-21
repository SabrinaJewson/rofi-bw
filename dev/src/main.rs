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
    let mut manifest_dir = fs::PathBuf::from(manifest_dir);
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
        .args(["--profile", profile_name])
        .status()
        .context("failed to spawn Cargo")?;

    anyhow::ensure!(status.success(), "Cargo failed");

    let target_base = fs::PathBuf::from_iter(["target", profile_dir_name]);

    copy_p(
        &target_base.join("librofi_bw_plugin.so"),
        fs::Path::new("build/lib/rofi-bw/plugin.so"),
    )?;
    copy_p(
        &target_base.join("rofi-bw"),
        fs::Path::new("build/bin/rofi-bw"),
    )?;
    copy_p(
        fs::Path::new("resources/bwi-font.ttf"),
        fs::Path::new("build/share/rofi-bw/bwi-font.ttf"),
    )?;
    let cards_dir = fs::Path::new("build/share/rofi-bw/cards");
    for card in fs::read_dir(fs::Path::new("resources/cards"))? {
        let card = card?;
        let path = card.path();
        copy_p(&path, &cards_dir.join(path.file_name().unwrap()))?;
    }

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

    let status = process::Command::new(fs::Path::new("build/bin/rofi-bw"))
        .env("ROFI_BW_LIB_DIR", fs::Path::new("build/lib/rofi-bw"))
        // Put a path that will always fail at start for extra testing
        .env("XDG_DATA_DIRS", "doesnt-exist:build/share")
        .args(args.rest)
        .status()
        .context("failed to spawn rofi-bw")?;

    anyhow::ensure!(status.success(), "rofi-bw failed");

    Ok(())
}

fn copy_p(src: &fs::Path, to: &fs::Path) -> anyhow::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, to)?;
    Ok(())
}

use anyhow::Context as _;
use clap::Parser as _;
use rofi_bw_util::fs;
use std::env;
use std::ffi::OsString;
use std::process;
