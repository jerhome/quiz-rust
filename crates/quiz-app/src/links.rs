// Opening the release page without egui-winit's "links" feature (no extra dependency).

use std::process::{Command, Stdio};

const RELEASES: &str = "https://github.com/jerhome/quiz-rust/releases/tag";

pub fn release_url(version: &str) -> String {
    format!("{RELEASES}/v{version}")
}

pub fn current_release_url() -> String {
    release_url(env!("CARGO_PKG_VERSION"))
}

// Launchers to try in order. On WSL, xdg-open is often missing: wslview, then the Windows shell.
fn launchers(url: &str) -> Vec<Command> {
    let cmd = |program: &str, args: &[&str]| {
        let mut c = Command::new(program);
        c.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        c
    };
    if cfg!(windows) {
        #[cfg_attr(not(windows), allow(unused_mut))]
        let mut c = cmd("cmd", &["/c", "start", "", url]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        vec![c]
    } else {
        vec![
            cmd("xdg-open", &[url]),
            cmd("wslview", &[url]),
            cmd("cmd.exe", &["/c", "start", "", url]),
        ]
    }
}

pub fn open_url(url: &str) -> std::io::Result<()> {
    let mut last = std::io::Error::other("no launcher");
    for mut command in launchers(url) {
        match command.spawn() {
            Ok(mut child) => {
                // Reap the child in the background so it does not linger as a zombie.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Ok(());
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_de_release() {
        assert_eq!(
            release_url("0.1.0"),
            "https://github.com/jerhome/quiz-rust/releases/tag/v0.1.0"
        );
        assert!(current_release_url().ends_with(&format!("/v{}", env!("CARGO_PKG_VERSION"))));
    }

    #[test]
    fn lanceurs_non_vides() {
        let l = launchers("https://example.org");
        assert!(!l.is_empty());
    }
}
