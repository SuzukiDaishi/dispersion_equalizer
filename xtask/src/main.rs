use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

const PLUGIN_PACKAGE: &str = "dispersion_equalizer";
const COMPONENT_NAME: &str = "Dispersion Equalizer.component";
const AUV2_ID: &str = "aufx:DsEQ:Zuky";

fn main() -> Result<(), Box<dyn Error>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if let Some(cmd) = args.first() {
        if cmd == "auv2" {
            return build_auv2(&args[1..]);
        }
    }

    nih_plug_xtask::main().map_err(Into::into)
}

fn build_auv2(extra_args: &[String]) -> Result<(), Box<dyn Error>> {
    if !cfg!(target_os = "macos") {
        return Err("`cargo auv2` is only supported on macOS".into());
    }

    let release = parse_release_profile(extra_args)?;
    let profile_dir = if release { "release" } else { "debug" };

    run_command(
        "cargo",
        &["xtask", "bundle", PLUGIN_PACKAGE, profile_flag(release)],
    )?;

    ensure_clap_wrapper_bundler()?;

    let dylib_path = format!("target/{profile_dir}/libdispersion_equalizer.dylib");
    let target_component = Path::new("target").join("bundled").join(COMPONENT_NAME);
    if target_component.exists() {
        std::fs::remove_dir_all(&target_component)?;
    }

    let expected_components = [
        Path::new("target").join(profile_dir).join(COMPONENT_NAME),
        Path::new(profile_dir).join(COMPONENT_NAME),
    ];
    for component in &expected_components {
        if component.exists() {
            std::fs::remove_dir_all(component)?;
        }
    }

    run_command("clap-wrapper-bundler", &["--auv2-id", AUV2_ID, &dylib_path])?;

    let component = expected_components
        .iter()
        .find(|path| path.exists())
        .cloned()
        .or_else(|| find_component_bundle(&Path::new("target").join(profile_dir)))
        .ok_or_else(|| {
            format!("Could not find generated AUv2 .component bundle for the {profile_dir} profile")
        })?;

    let bundled_dir = Path::new("target/bundled");
    std::fs::create_dir_all(bundled_dir)?;

    copy_dir_recursive(&component, &target_component)?;

    println!("Created AUv2 bundle at '{}'", target_component.display());
    Ok(())
}

fn parse_release_profile(extra_args: &[String]) -> Result<bool, Box<dyn Error>> {
    let mut release = true;
    let mut saw_release = false;
    let mut saw_debug = false;

    for arg in extra_args {
        match arg.as_str() {
            "--release" => {
                release = true;
                saw_release = true;
            }
            "--debug" => {
                release = false;
                saw_debug = true;
            }
            "--help" | "-h" => {
                return Err("Usage: cargo auv2 [--release|--debug]\n\n`cargo auv2` builds release AUv2 bundles by default so Logic Pro sees the same optimized arm64 slice that is merged with the x86_64 release slice.".into());
            }
            other => {
                return Err(format!("Unknown argument for `cargo auv2`: {other}").into());
            }
        }
    }

    if saw_release && saw_debug {
        return Err("`cargo auv2` cannot use both --release and --debug".into());
    }

    Ok(release)
}

fn ensure_clap_wrapper_bundler() -> Result<(), Box<dyn Error>> {
    let has_bundler = Command::new("clap-wrapper-bundler")
        .arg("--help")
        .output()
        .is_ok();

    if has_bundler {
        return Ok(());
    }

    run_command(
        "cargo",
        &[
            "install",
            "--git",
            "https://github.com/blepfx/clap-wrapper-rs.git",
            "clap-wrapper-bundler",
            "--locked",
        ],
    )
}

fn profile_flag(release: bool) -> &'static str {
    if release {
        "--release"
    } else {
        ""
    }
}

fn run_command(program: &str, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let filtered_args = args
        .iter()
        .copied()
        .filter(|arg| !arg.is_empty())
        .collect::<Vec<_>>();

    let status = Command::new(program).args(&filtered_args).status()?;
    if !status.success() {
        return Err(format!("Command failed: {} {}", program, filtered_args.join(" ")).into());
    }
    Ok(())
}

fn find_component_bundle(root: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("component") {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_component_bundle(&path) {
                return Some(found);
            }
        }
    }
    None
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), Box<dyn Error>> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_release_profile;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn auv2_defaults_to_release() {
        assert!(parse_release_profile(&[]).unwrap());
    }

    #[test]
    fn auv2_accepts_explicit_release() {
        assert!(parse_release_profile(&args(&["--release"])).unwrap());
    }

    #[test]
    fn auv2_accepts_debug_override() {
        assert!(!parse_release_profile(&args(&["--debug"])).unwrap());
    }

    #[test]
    fn auv2_rejects_conflicting_profiles() {
        assert!(parse_release_profile(&args(&["--release", "--debug"])).is_err());
    }

    #[test]
    fn auv2_rejects_unknown_flags() {
        assert!(parse_release_profile(&args(&["--profile", "release"])).is_err());
    }
}
