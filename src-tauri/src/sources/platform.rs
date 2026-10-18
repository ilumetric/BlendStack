//! What "the current platform" means for each download server.

use regex::Regex;
use std::sync::LazyLock;

pub struct Target {
    pub builder_os: &'static str,
    pub builder_arch: &'static str,
    pub builder_ext: &'static str,
    /// Matches release archive names; capture 1 is the version.
    pub release_re: Regex,
}

const VERSION: &str = r"(\d+\.\d+(?:\.\d+)?[a-z]?)";

static TARGET: LazyLock<Target> = LazyLock::new(|| {
    let arm = cfg!(target_arch = "aarch64");
    if cfg!(target_os = "windows") {
        Target {
            builder_os: "windows",
            builder_arch: if arm { "arm64" } else { "amd64" },
            builder_ext: "zip",
            release_re: Regex::new(&if arm {
                format!(r"^blender-{VERSION}-windows-arm64\.zip$")
            } else {
                format!(r"^blender-{VERSION}-windows(?:-x64|64)\.zip$")
            })
            .unwrap(),
        }
    } else if cfg!(target_os = "macos") {
        Target {
            builder_os: "darwin",
            builder_arch: if arm { "arm64" } else { "x86_64" },
            builder_ext: "dmg",
            release_re: Regex::new(&if arm {
                format!(r"^blender-{VERSION}-macos-arm64\.dmg$")
            } else {
                format!(r"^blender-{VERSION}-(?:macos-x64|macOS)\.dmg$")
            })
            .unwrap(),
        }
    } else {
        Target {
            builder_os: "linux",
            builder_arch: if arm { "aarch64" } else { "x86_64" },
            builder_ext: "xz",
            release_re: Regex::new(&format!(
                r"^blender-{VERSION}-linux(?:-x64|64|-glibc\d+-x86_64)\.tar\.(?:xz|bz2)$"
            ))
            .unwrap(),
        }
    }
});

pub fn current() -> &'static Target {
    &TARGET
}

#[cfg(all(test, target_os = "windows", target_arch = "x86_64"))]
mod tests {
    #[test]
    fn matches_windows_archives() {
        let re = &super::current().release_re;
        assert!(re.is_match("blender-5.1.2-windows-x64.zip"));
        assert!(re.is_match("blender-2.83.20-windows64.zip"));
        assert!(!re.is_match("blender-5.1.2-windows-x64.msi"));
        assert!(!re.is_match("blender-5.1.2-windows-arm64.zip"));
    }
}
