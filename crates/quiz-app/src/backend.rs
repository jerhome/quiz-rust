// Display backend selection (Linux). winit forbids recreating an event loop in the same
// process, so the backend is chosen before eframe starts and a change needs a restart.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Deserializer, Serialize};

pub const ENV_VAR: &str = "QUIZ_RUST_BACKEND";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    X11,
    Wayland,
}

impl Backend {
    pub fn parse(s: &str) -> Option<Backend> {
        match s.trim().to_ascii_lowercase().as_str() {
            "x11" => Some(Backend::X11),
            "wayland" => Some(Backend::Wayland),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Backend::X11 => "X11",
            Backend::Wayland => "Wayland",
        }
    }

    pub const ALL: [Backend; 2] = [Backend::X11, Backend::Wayland];
}

// Unknown values in config.toml must not discard the other settings: they mean X11.
pub fn deserialize_lenient<'de, D: Deserializer<'de>>(d: D) -> Result<Backend, D::Error> {
    let s = String::deserialize(d)?;
    Ok(Backend::parse(&s).unwrap_or_default())
}

/// Backend actually in use for this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Active {
    pub backend: Backend,
    /// Wayland was requested but unavailable, X11 is used instead.
    pub fell_back: bool,
}

// Priority: QUIZ_RUST_BACKEND (troubleshooting) > saved setting > X11.
// Wayland without a WAYLAND_DISPLAY cannot work: fall back to X11.
pub fn resolve(env: Option<&str>, setting: Backend, wayland_display: Option<&str>) -> Active {
    let requested = env.and_then(Backend::parse).unwrap_or(setting);
    let has_display = wayland_display.is_some_and(|d| !d.is_empty());
    if requested == Backend::Wayland && !has_display {
        return Active {
            backend: Backend::X11,
            fell_back: true,
        };
    }
    Active {
        backend: requested,
        fell_back: false,
    }
}

/// Native decorations stay on X11 only: Windows and Wayland (client-side decorations that
/// clash with the theme) get our own title bar instead.
pub fn custom_titlebar(windows: bool, backend: Backend) -> bool {
    windows || backend == Backend::Wayland
}

/// Whether the selected backend differs from what the running process uses.
pub fn restart_needed(selected: Backend, active: Active) -> bool {
    if active.fell_back && selected == Backend::Wayland {
        return false; // restarting would fall back again
    }
    selected != active.backend
}

// `force`: pin the backend through the environment (fallback relaunch); otherwise the
// variable is removed so the saved setting applies to the new instance.
pub fn relaunch_command(exe: &Path, args: Vec<OsString>, force: Option<Backend>) -> Command {
    let mut cmd = Command::new(exe);
    cmd.args(args);
    match force {
        Some(b) => cmd.env(ENV_VAR, b.label().to_ascii_lowercase()),
        None => cmd.env_remove(ENV_VAR),
    };
    cmd
}

pub fn spawn_relaunch(force: Option<Backend>) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let args = std::env::args_os().skip(1).collect();
    relaunch_command(&exe, args, force).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priorite_env_reglage_defaut() {
        let wl = Some("wayland-0");
        assert_eq!(resolve(None, Backend::X11, wl).backend, Backend::X11);
        assert_eq!(
            resolve(None, Backend::Wayland, wl).backend,
            Backend::Wayland
        );
        assert_eq!(
            resolve(Some("x11"), Backend::Wayland, wl).backend,
            Backend::X11
        );
        assert_eq!(
            resolve(Some("WAYLAND"), Backend::X11, wl).backend,
            Backend::Wayland
        );
        // invalid value in the environment: ignored
        assert_eq!(
            resolve(Some("mir"), Backend::Wayland, wl).backend,
            Backend::Wayland
        );
        assert_eq!(resolve(Some(""), Backend::X11, wl).backend, Backend::X11);
    }

    #[test]
    fn repli_sans_wayland_display() {
        for wl in [None, Some("")] {
            let a = resolve(None, Backend::Wayland, wl);
            assert_eq!(
                a,
                Active {
                    backend: Backend::X11,
                    fell_back: true
                }
            );
        }
        assert!(!resolve(None, Backend::X11, None).fell_back);
    }

    #[test]
    fn barre_de_titre_personnalisee() {
        assert!(!custom_titlebar(false, Backend::X11));
        assert!(custom_titlebar(false, Backend::Wayland));
        assert!(custom_titlebar(true, Backend::X11));
    }

    #[test]
    fn redemarrage_necessaire() {
        let x11 = Active {
            backend: Backend::X11,
            fell_back: false,
        };
        let wl = Active {
            backend: Backend::Wayland,
            fell_back: false,
        };
        let fb = Active {
            backend: Backend::X11,
            fell_back: true,
        };
        assert!(!restart_needed(Backend::X11, x11));
        assert!(restart_needed(Backend::Wayland, x11));
        assert!(restart_needed(Backend::X11, wl));
        assert!(!restart_needed(Backend::Wayland, fb));
        assert!(!restart_needed(Backend::X11, fb));
    }

    #[test]
    fn commande_de_relance() {
        let args = vec![OsString::from("--a")];
        let forced = relaunch_command(Path::new("/bin/app"), args.clone(), Some(Backend::X11));
        assert_eq!(forced.get_program(), "/bin/app");
        assert_eq!(forced.get_args().collect::<Vec<_>>(), ["--a"]);
        let env: Vec<_> = forced.get_envs().collect();
        assert_eq!(
            env,
            [(
                std::ffi::OsStr::new(ENV_VAR),
                Some(std::ffi::OsStr::new("x11"))
            )]
        );
        let plain = relaunch_command(Path::new("/bin/app"), args, None);
        let env: Vec<_> = plain.get_envs().collect();
        assert_eq!(env, [(std::ffi::OsStr::new(ENV_VAR), None)]);
    }

    #[test]
    fn analyse_et_serialisation() {
        assert_eq!(Backend::parse(" Wayland "), Some(Backend::Wayland));
        assert_eq!(Backend::parse("mir"), None);
        let s = toml::to_string(&Wrap {
            display_backend: Backend::Wayland,
        })
        .unwrap();
        assert!(s.contains("\"wayland\""));
        let w: Wrap = toml::from_str("display_backend = \"wayland\"").unwrap();
        assert_eq!(w.display_backend, Backend::Wayland);
        let w: Wrap = toml::from_str("display_backend = \"inconnu\"").unwrap();
        assert_eq!(w.display_backend, Backend::X11);
    }

    #[derive(Serialize, Deserialize)]
    struct Wrap {
        #[serde(deserialize_with = "deserialize_lenient")]
        display_backend: Backend,
    }
}
