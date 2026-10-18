//! Starting Blender.

use crate::error::{msg, Result};
use crate::model::InstalledBuild;
use crate::settings::Settings;
use std::path::Path;
use std::process::{Command, Stdio};

/// Splits an argument string the way a shell would (quotes group words).
pub fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    for c in s.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                has = true;
            }
            (None, c) if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        args.push(cur);
    }
    args
}

pub fn launch(build: &InstalledBuild, settings: &Settings, file: Option<&str>) -> Result<()> {
    let exe = Path::new(&build.executable);
    if !exe.is_file() {
        return Err(msg(
            "Blender executable not found — the build may have been moved or deleted",
        ));
    }
    let mut args = split_args(&settings.global_args);
    if let Some(extra) = settings.build_args.get(&build.id) {
        args.extend(split_args(extra));
    }
    if let Some(f) = file {
        if !Path::new(f).is_file() {
            return Err(msg(format!("file not found: {f}")));
        }
        args.push(f.to_string());
    }

    let mut cmd = Command::new(program(exe, settings));
    cmd.args(&args)
        .current_dir(exe.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        let console = if settings.show_console {
            CREATE_NEW_CONSOLE
        } else {
            DETACHED_PROCESS
        };
        cmd.creation_flags(console | CREATE_NEW_PROCESS_GROUP);
    }

    cmd.spawn().map_err(|e| match e.raw_os_error() {
        // ERROR_SXS_CANT_GEN_ACTCTX: Windows refused to load Blender's bundled runtime from this folder.
        Some(14001) => msg(
            "Windows blocked Blender from starting in this folder (side-by-side error).              Move the library to another folder in Settings → Library.",
        ),
        _ => msg(format!("could not start Blender: {e}")),
    })?;
    Ok(())
}

/// On Windows `blender-launcher.exe` starts Blender without a console window.
fn program(exe: &Path, settings: &Settings) -> std::path::PathBuf {
    if cfg!(windows) && !settings.show_console {
        let launcher = exe.with_file_name("blender-launcher.exe");
        if launcher.is_file() {
            return launcher;
        }
    }
    exe.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::split_args;

    #[test]
    fn splits_quoted_args() {
        assert_eq!(
            split_args(r#"--factory-startup  -p 0 0 "a b" ''"#),
            vec!["--factory-startup", "-p", "0", "0", "a b", ""]
        );
        assert!(split_args("   ").is_empty());
    }
}
