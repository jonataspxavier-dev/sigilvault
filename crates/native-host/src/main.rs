//! `sigilvault-host.exe`: o navegador abre este programa para o Native Messaging. Ele só repassa
//! mensagens entre a extensão (stdin/stdout) e o app da bandeja (pipe protegido). O conteúdo vem
//! criptografado de ponta a ponta, então ele não consegue ler nada.
//!
//! `sigilvault-host.exe`: the browser starts this program for Native Messaging. It only relays
//! messages between the extension (stdin/stdout) and the tray app (protected pipe). The content
//! is end-to-end encrypted, so it cannot read anything.

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    host::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("SigilVault só roda no Windows / SigilVault only runs on Windows");
}

#[cfg(windows)]
mod host {
    use std::io::{self, Write};
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::{Command, ExitCode};
    use std::time::{Duration, Instant};

    use sigilvault_ipc::pipe::{self, Pipe};
    use sigilvault_ipc::{APP_EXE, MAX_MESSAGE, frame, process, signature};
    use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows_sys::Win32::System::Threading::{CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};

    /// Quanto esperar o app abrir. / How long to wait for the app to start.
    const LAUNCH_WAIT: Duration = Duration::from_secs(20);

    /// Quem chamou, segundo o navegador (nunca segundo a página).
    /// Who called, according to the browser (never according to the page).
    /// - Chrome/Brave/Opera: `chrome-extension://<id>/` (1º argumento / 1st argument).
    /// - Firefox: caminho do manifesto + id da extensão (2º argumento). / manifest path + extension id (2nd argument).
    fn caller(args: &[String]) -> Option<String> {
        let first = args.get(1)?;
        if first.starts_with("chrome-extension://") {
            return Some(first.clone());
        }
        args.get(2).filter(|id| !id.starts_with("--")).map(|id| format!("firefox:{id}"))
    }

    /// Avisa a extensão de um problema e encerra. / Tells the extension about a problem and exits.
    fn fail(code: &str) -> ExitCode {
        let msg = serde_json::json!({ "t": "host_error", "code": code }).to_string();
        let _ = frame::write(io::stdout().lock(), msg.as_bytes(), MAX_MESSAGE);
        ExitCode::FAILURE
    }

    /// Abre o app na bandeja, fora do "job" do navegador (para não fechar junto com ele).
    /// Starts the app in the tray, outside the browser's job (so it does not close with it).
    fn launch_app(app: &PathBuf) -> io::Result<()> {
        let base = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
        Command::new(app)
            .arg("--tray")
            .creation_flags(base | CREATE_BREAKAWAY_FROM_JOB)
            .spawn()
            .or_else(|_| Command::new(app).arg("--tray").creation_flags(base).spawn())
            .map(drop)
    }

    /// Conecta ao app; se ele não estiver aberto, abre e espera. / Connects to the app; if not running, starts it and waits.
    fn connect(name: &str, app: &PathBuf) -> io::Result<Pipe> {
        match Pipe::connect(name, Duration::from_secs(5)) {
            Err(e) if e.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32) => {
                launch_app(app)?;
                let deadline = Instant::now() + LAUNCH_WAIT;
                loop {
                    std::thread::sleep(Duration::from_millis(250));
                    match Pipe::connect(name, Duration::from_secs(5)) {
                        Ok(p) => return Ok(p),
                        Err(e) if Instant::now() >= deadline => return Err(e),
                        Err(_) => {}
                    }
                }
            }
            other => other,
        }
    }

    pub fn run() -> ExitCode {
        let args: Vec<String> = std::env::args().collect();
        let Some(caller) = caller(&args) else { return fail("bad_caller") };
        let (Ok(me), Ok(app), Ok(name)) = (std::env::current_exe(), process::sibling(APP_EXE), pipe::default_name()) else {
            return fail("app_unavailable");
        };
        let Ok(pipe) = connect(&name, &app) else { return fail("app_unavailable") };

        // Do outro lado precisa estar o nosso app (mesma pasta e mesma assinatura).
        // Our app must be on the other side (same folder and same signature).
        let trusted = pipe
            .peer_pid()
            .and_then(process::image_path)
            .is_ok_and(|peer| process::same_file(&peer, &app) && signature::peer_allowed(&me, &peer));
        if !trusted {
            return fail("untrusted_app");
        }

        let hello = serde_json::json!({ "t": "host", "v": 1, "caller": caller }).to_string();
        if frame::write(&pipe, hello.as_bytes(), MAX_MESSAGE).is_err() {
            return fail("app_unavailable");
        }

        let pipe = std::sync::Arc::new(pipe);
        // Navegador → app. Quando o navegador fecha o stdin, encerra tudo.
        // Browser → app. When the browser closes stdin, everything ends.
        let to_app = {
            let pipe = pipe.clone();
            std::thread::spawn(move || {
                let mut stdin = io::stdin().lock();
                while let Ok(Some(msg)) = frame::read(&mut stdin, MAX_MESSAGE) {
                    if frame::write(&*pipe, &msg, MAX_MESSAGE).is_err() {
                        break;
                    }
                }
                pipe.cancel();
            })
        };
        // App → navegador. / App → browser.
        let mut stdout = io::stdout().lock();
        while let Ok(Some(msg)) = frame::read(&*pipe, MAX_MESSAGE) {
            if frame::write(&mut stdout, &msg, MAX_MESSAGE).is_err() {
                break;
            }
        }
        let _ = stdout.flush();
        drop(to_app);
        ExitCode::SUCCESS
    }

    #[cfg(test)]
    mod tests {
        use super::caller;

        fn args(v: &[&str]) -> Vec<String> {
            v.iter().map(|s| s.to_string()).collect()
        }

        #[test]
        fn reads_caller_from_browser_arguments() {
            assert_eq!(
                caller(&args(&["host.exe", "chrome-extension://abcdefghijklmnopabcdefghijklmnop/", "--parent-window=123"])).as_deref(),
                Some("chrome-extension://abcdefghijklmnopabcdefghijklmnop/")
            );
            assert_eq!(
                caller(&args(&["host.exe", r"C:\x\app.sigilvault.host.json", "sigilvault@sigilvault.app"])).as_deref(),
                Some("firefox:sigilvault@sigilvault.app")
            );
            assert_eq!(caller(&args(&["host.exe"])), None);
            assert_eq!(caller(&args(&["host.exe", "qualquer", "--parent-window=1"])), None);
        }
    }
}
