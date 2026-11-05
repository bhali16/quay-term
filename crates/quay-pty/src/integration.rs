//! Shell integration for local sessions: prompt and command marks (OSC 133) without
//! touching the user's dotfiles. zsh is started through a `ZDOTDIR` shim that sources the
//! real startup files; bash gets a one-shot `PROMPT_COMMAND` bootstrap. Other shells are left
//! alone (fish 4 and newer emit the marks themselves).

use std::fs;
use std::io;
use std::path::Path;

#[cfg(unix)]
use portable_pty::CommandBuilder;

const FILES: &[(&str, &str)] = &[
    ("quay.zsh", include_str!("../shell/quay.zsh")),
    ("quay.bash", include_str!("../shell/quay.bash")),
    ("zsh/.zshenv", include_str!("../shell/zsh/.zshenv")),
    ("zsh/.zprofile", include_str!("../shell/zsh/.zprofile")),
    ("zsh/.zshrc", include_str!("../shell/zsh/.zshrc")),
    ("zsh/.zlogin", include_str!("../shell/zsh/.zlogin")),
];

/// Write the scripts under `dir`, replacing only files whose content changed.
pub fn install(dir: &Path) -> io::Result<()> {
    for (name, body) in FILES {
        let path = dir.join(name);
        if fs::read_to_string(&path).is_ok_and(|old| old == *body) {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, body)?;
    }
    Ok(())
}

/// Single-quote `s` for a POSIX shell.
#[cfg(unix)]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Adjust how `shell` starts so it loads the integration. Does nothing for other shells.
#[cfg(unix)]
pub(crate) fn configure(cmd: &mut CommandBuilder, shell: &str, dir: &Path) {
    let name = Path::new(shell)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    match name {
        "zsh" => {
            if let Some(orig) = std::env::var_os("ZDOTDIR") {
                cmd.env("QUAY_ORIG_ZDOTDIR", orig);
            }
            cmd.env("ZDOTDIR", dir.join("zsh"));
            cmd.env("QUAY_SHELL_INTEGRATION_DIR", dir);
        }
        "bash" => {
            let boot = format!(". {}", sh_quote(&dir.join("quay.bash").to_string_lossy()));
            let command = match std::env::var("PROMPT_COMMAND") {
                Ok(orig) if !orig.trim().is_empty() => format!("{boot};{orig}"),
                _ => boot,
            };
            cmd.env("PROMPT_COMMAND", command);
            cmd.env("QUAY_SHELL_INTEGRATION_DIR", dir);
        }
        _ => {}
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Command;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("quay-pty-int-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn install_is_idempotent() {
        let d = scratch("install");
        install(&d).unwrap();
        let first = fs::metadata(d.join("quay.zsh"))
            .unwrap()
            .modified()
            .unwrap();
        install(&d).unwrap();
        assert_eq!(
            first,
            fs::metadata(d.join("quay.zsh"))
                .unwrap()
                .modified()
                .unwrap()
        );
        assert!(d.join("zsh/.zshrc").exists());
        let _ = fs::remove_dir_all(&d);
    }

    /// Runs a real interactive zsh through the shim and checks the marks reach the output,
    /// that the user's own .zshrc still ran, and that ZDOTDIR is not left pointing at the shim.
    #[test]
    fn zsh_shim_loads_user_rc_and_emits_marks() {
        if !Path::new("/bin/zsh").exists() {
            return;
        }
        let d = scratch("zsh");
        let home = d.join("home");
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join(".zshrc"), "export QUAY_TEST_RC=ran\n").unwrap();
        let shim = d.join("shim");
        install(&shim).unwrap();
        let out = Command::new("/bin/zsh")
            .args(["-i", "-c", "print -r -- \"rc=$QUAY_TEST_RC zdot=${ZDOTDIR-unset}\"; print -r -- \"${precmd_functions[*]}\""])
            .env("HOME", &home)
            .env_remove("ZDOTDIR")
            .env("ZDOTDIR", shim.join("zsh"))
            .env("QUAY_SHELL_INTEGRATION_DIR", &shim)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("rc=ran"), "user .zshrc did not run: {text}");
        assert!(text.contains("zdot=unset"), "ZDOTDIR leaked: {text}");
        assert!(text.contains("__quay_precmd"), "hooks missing: {text}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn bash_bootstrap_installs_hooks() {
        if !Path::new("/bin/bash").exists() {
            return;
        }
        let d = scratch("bash");
        install(&d).unwrap();
        let boot = format!(". {}", sh_quote(&d.join("quay.bash").to_string_lossy()));
        let out = Command::new("/bin/bash")
            .args([
                "--norc",
                "-i",
                "-c",
                ". \"$QUAY_SHELL_INTEGRATION_DIR/quay.bash\"; echo \"[$PROMPT_COMMAND]\"",
            ])
            .env("PROMPT_COMMAND", format!("{boot};echo orig"))
            .env("QUAY_SHELL_INTEGRATION_DIR", &d)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.contains("[__quay_prompt;echo orig;__quay_prompt_end]"),
            "unexpected PROMPT_COMMAND: {text}"
        );
        let _ = fs::remove_dir_all(&d);
    }
}
