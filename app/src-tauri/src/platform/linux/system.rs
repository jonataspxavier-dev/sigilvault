//! Funções do sistema: inatividade e endurecimento do processo.
//! System functions: idle time and process hardening.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// De quanto em quanto tempo medir a inatividade. / How often to measure idle time.
const SAMPLE_EVERY: Duration = Duration::from_secs(2);

/// Última medida (ms) e quando foi tirada. / Last sample (ms) and when it was taken.
struct Sample {
    idle_ms: AtomicU64,
    taken_at: std::sync::Mutex<Instant>,
}

static SAMPLE: OnceLock<Sample> = OnceLock::new();

/// Pergunta ao ambiente há quanto tempo ninguém usa o teclado ou o mouse. Tenta o GNOME (Mutter)
/// e depois o protetor de tela do freedesktop (KDE e outros).
/// Asks the desktop how long nobody has used the keyboard or mouse. Tries GNOME (Mutter) and then
/// the freedesktop screensaver (KDE and others).
async fn query(conn: &zbus::Connection) -> Option<u64> {
    if let Ok(reply) = conn
        .call_method(Some("org.gnome.Mutter.IdleMonitor"), "/org/gnome/Mutter/IdleMonitor/Core", Some("org.gnome.Mutter.IdleMonitor"), "GetIdletime", &())
        .await
    {
        return reply.body().deserialize::<u64>().ok();
    }
    let reply = conn
        .call_method(Some("org.freedesktop.ScreenSaver"), "/org/freedesktop/ScreenSaver", Some("org.freedesktop.ScreenSaver"), "GetSessionIdleTime", &())
        .await
        .ok()?;
    reply.body().deserialize::<u32>().ok().map(u64::from)
}

fn sample() -> &'static Sample {
    SAMPLE.get_or_init(|| {
        let _ = std::thread::Builder::new().name("sigilvault-idle".into()).spawn(|| {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
            runtime.block_on(async {
                let Ok(conn) = zbus::Connection::session().await else { return };
                loop {
                    if let Some(ms) = query(&conn).await
                        && let Some(s) = SAMPLE.get()
                    {
                        s.idle_ms.store(ms, Ordering::Relaxed);
                        *s.taken_at.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
                    }
                    tokio::time::sleep(SAMPLE_EVERY).await;
                }
            });
        });
        Sample { idle_ms: AtomicU64::new(0), taken_at: std::sync::Mutex::new(Instant::now()) }
    })
}

/// Milissegundos desde o último uso de teclado ou mouse. Vem do GNOME ou do protetor de tela do
/// freedesktop; sem nenhum dos dois, fica em 0 e o bloqueio por inatividade não dispara (o
/// bloqueio por tela travada e suspensão continua valendo).
/// Milliseconds since the last keyboard or mouse input. Comes from GNOME or the freedesktop
/// screensaver; with neither, it stays 0 and idle auto-lock does not fire (lock on screen lock and
/// suspend still works).
pub fn idle_millis() -> u64 {
    let s = sample();
    let base = s.idle_ms.load(Ordering::Relaxed);
    if base == 0 {
        return 0;
    }
    // Soma o tempo desde a medida: a medida tem até 2 s de idade. / Adds the time since the sample: it is up to 2 s old.
    let age = s.taken_at.lock().unwrap_or_else(|e| e.into_inner()).elapsed();
    base.saturating_add(age.as_millis() as u64)
}

/// Sem relatórios de erro com dump de memória: no Linux, desliga os despejos de memória do processo
/// (`RLIMIT_CORE` zero). Não usa `PR_SET_DUMPABLE`, porque ele impediria o host de ler `/proc/<pid>/exe`.
/// No error reports with memory dumps: on Linux, turns off the process's core dumps (`RLIMIT_CORE`
/// zero). Does not use `PR_SET_DUMPABLE`, because it would stop the host from reading `/proc/<pid>/exe`.
pub fn exclude_from_error_reporting() {
    let zero = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: `zero` é válido para leitura durante a chamada. / `zero` is valid for reading during the call.
    unsafe { libc::setrlimit(libc::RLIMIT_CORE, &zero) };
}

/// Em máquinas com placa NVIDIA (driver proprietário), o WebKitGTK tem um problema conhecido no
/// renderizador `DMABUF`: a janela pode travar ou crescer a memória sem limite (já vimos o processo
/// da janela chegar a 5,6 GB e abortar). Nesses casos desliga esse renderizador; o desenho fica um
/// pouco menos acelerado, mas estável. Quem já definiu a variável mantém a escolha.
/// On machines with an NVIDIA card (proprietary driver), WebKitGTK has a known problem in the
/// `DMABUF` renderer: the window may hang or grow memory without limit (we have seen the window's
/// process reach 5.6 GB and abort). In those cases it turns that renderer off; drawing is a bit
/// less accelerated, but stable. Whoever already set the variable keeps their choice.
pub fn harden_webview_env() {
    if needs_dmabuf_workaround(std::path::Path::new("/proc/driver/nvidia").is_dir(), std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some()) {
        // SAFETY: chamado no início de `run()`, antes de o app criar outras threads; ninguém mais lê o
        // ambiente nesse momento. / Called at the start of `run()`, before the app creates other threads;
        // nobody else reads the environment at that moment.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
}

/// Decide se aplica o contorno: há placa NVIDIA e a pessoa não escolheu nada. / Decides whether to apply the workaround: there is an NVIDIA card and the person chose nothing.
fn needs_dmabuf_workaround(nvidia_driver: bool, already_set: bool) -> bool {
    nvidia_driver && !already_set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workaround_only_with_nvidia_and_no_user_choice() {
        assert!(needs_dmabuf_workaround(true, false));
        assert!(!needs_dmabuf_workaround(true, true));
        assert!(!needs_dmabuf_workaround(false, false));
        assert!(!needs_dmabuf_workaround(false, true));
    }
}
