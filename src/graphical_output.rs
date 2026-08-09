use std::{
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GraphicalOutputBackend {
    candidates: Vec<Backend>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Backend {
    Kitten(PathBuf),
    Kitty(PathBuf),
    WezTerm(PathBuf),
    Chafa(PathBuf),
}

impl GraphicalOutputBackend {
    pub(crate) fn detect() -> Option<Self> {
        if !std::io::stdout().is_terminal() {
            return None;
        }
        let candidates = Self::detect_on_path(env::var_os("PATH").as_deref());
        (!candidates.is_empty()).then_some(Self { candidates })
    }

    fn detect_on_path(path: Option<&std::ffi::OsStr>) -> Vec<Backend> {
        [
            ("kitten", Backend::Kitten as fn(PathBuf) -> Backend),
            ("kitty", Backend::Kitty as fn(PathBuf) -> Backend),
            ("wezterm", Backend::WezTerm as fn(PathBuf) -> Backend),
            ("chafa", Backend::Chafa as fn(PathBuf) -> Backend),
        ]
        .into_iter()
        .filter_map(|(name, backend)| find_executable(name, path).map(backend))
        .collect()
    }

    pub(crate) fn display_svg(&self, svg: &str) -> Result<()> {
        let file = TemporarySvg::create(svg)?;
        let mut failures = Vec::new();
        for backend in &self.candidates {
            match backend.display(file.path()) {
                Ok(()) => return Ok(()),
                Err(err) => failures.push(format!("{}: {err:#}", backend.name())),
            }
        }
        bail!("{}", failures.join("; "))
    }
}

impl Backend {
    fn display(&self, path: &Path) -> Result<()> {
        let mut command = match self {
            Self::Kitten(program) => {
                let mut command = Command::new(program);
                command.args(["icat", "--stdin=no"]);
                command
            }
            Self::Kitty(program) => {
                let mut command = Command::new(program);
                command.args(["+kitten", "icat", "--stdin=no"]);
                command
            }
            Self::WezTerm(program) => {
                let mut command = Command::new(program);
                command.arg("imgcat");
                command
            }
            Self::Chafa(program) => Command::new(program),
        };
        let status = command
            .arg(path)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("failed to start process")?;
        if !status.success() {
            bail!("exited with {status}");
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Kitten(_) => "kitten icat",
            Self::Kitty(_) => "kitty +kitten icat",
            Self::WezTerm(_) => "wezterm imgcat",
            Self::Chafa(_) => "chafa",
        }
    }
}

fn find_executable(name: &str, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let path = path?;
    executable_names(name).into_iter().find_map(|name| {
        env::split_paths(path)
            .map(|directory| directory.join(&name))
            .find(|candidate| is_executable(candidate))
    })
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn executable_names(name: &str) -> Vec<OsString> {
    #[cfg(windows)]
    {
        let extensions = env::var_os("PATHEXT").unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into());
        let mut names = vec![OsString::from(name)];
        names.extend(
            extensions
                .to_string_lossy()
                .split(';')
                .filter(|extension| !extension.is_empty())
                .map(|extension| OsString::from(format!("{name}{extension}"))),
        );
        names
    }
    #[cfg(not(windows))]
    {
        vec![OsString::from(name)]
    }
}

struct TemporarySvg {
    path: PathBuf,
}

impl TemporarySvg {
    fn create(svg: &str) -> Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for attempt in 0..100 {
            let path = env::temp_dir().join(format!(
                "wolfie-{}-{nonce}-{attempt}.svg",
                std::process::id()
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    file.write_all(svg.as_bytes()).with_context(|| {
                        format!("failed to write temporary SVG {}", path.display())
                    })?;
                    return Ok(Self { path });
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!("failed to create temporary SVG {}", path.display())
                    });
                }
            }
        }
        bail!("failed to allocate a unique temporary SVG file")
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporarySvg {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_backends_in_priority_order() {
        let directory = tempfile_directory();
        fs::write(directory.join("chafa"), "").unwrap();
        fs::write(directory.join("kitten"), "").unwrap();
        make_executable(&directory.join("chafa"));
        make_executable(&directory.join("kitten"));

        let backends = GraphicalOutputBackend::detect_on_path(Some(directory.as_os_str()));
        assert!(matches!(backends.first(), Some(Backend::Kitten(_))));
        assert!(matches!(backends.get(1), Some(Backend::Chafa(_))));

        fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn creates_and_removes_temporary_svg() {
        let path = {
            let file = TemporarySvg::create("<svg></svg>").unwrap();
            assert_eq!(fs::read_to_string(file.path()).unwrap(), "<svg></svg>");
            file.path().to_owned()
        };
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn falls_through_when_an_earlier_backend_fails() {
        let directory = tempfile_directory();
        let failing_backend = directory.join("fails");
        let succeeding_backend = directory.join("succeeds");
        fs::write(&failing_backend, "#!/bin/sh\nexit 1\n").unwrap();
        fs::write(&succeeding_backend, "#!/bin/sh\nexit 0\n").unwrap();
        make_executable(&failing_backend);
        make_executable(&succeeding_backend);

        let backend = GraphicalOutputBackend {
            candidates: vec![
                Backend::Kitten(failing_backend),
                Backend::Chafa(succeeding_backend),
            ],
        };

        backend.display_svg("<svg></svg>").unwrap();
        fs::remove_dir_all(directory).ok();
    }

    fn tempfile_directory() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!(
            "wolfie-graphical-output-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        directory
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}
}
