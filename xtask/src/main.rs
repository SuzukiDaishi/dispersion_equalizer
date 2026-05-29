use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

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

    let release = extra_args.iter().any(|arg| arg == "--release");
    let profile_dir = if release { "release" } else { "debug" };

    run_command(
        "cargo",
        &["xtask", "bundle", "dispersion_equalizer", profile_flag(release)],
    )?;

    ensure_clap_wrapper_bundler()?;

    let dylib_path = format!("target/{profile_dir}/libdispersion_equalizer.dylib");
    let generated_component = Path::new(profile_dir).join("Dispersion Equalizer.component");
    if generated_component.exists() {
        std::fs::remove_dir_all(&generated_component)?;
    }

    run_command(
        "clap-wrapper-bundler",
        &["--auv2-id", "aufx:DsEQ:Zuky", &dylib_path],
    )?;

    let component = if generated_component.exists() {
        generated_component
    } else {
        find_component_bundle(Path::new("target"))
            .ok_or("Could not find generated AUv2 .component bundle")?
    };

    let bundled_dir = Path::new("target/bundled");
    std::fs::create_dir_all(bundled_dir)?;

    let target_component = bundled_dir.join(
        component
            .file_name()
            .ok_or("Invalid component path returned by bundler")?,
    );

    if target_component.exists() {
        std::fs::remove_dir_all(&target_component)?;
    }
    copy_dir_recursive(&component, &target_component)?;

    println!("Created AUv2 bundle at '{}'", target_component.display());
    Ok(())
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
        return Err(
            format!("Command failed: {} {}", program, filtered_args.join(" ")).into(),
        );
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
