//! Funções do sistema: inatividade e endurecimento do processo.
//! System functions: idle time and process hardening.

/// Milissegundos desde o último uso de teclado ou mouse. Ainda não implementado no Linux
/// (sempre 0: o bloqueio por inatividade não dispara).
/// Milliseconds since the last keyboard or mouse input. Not implemented on Linux yet (always 0:
/// idle auto-lock never fires).
pub fn idle_millis() -> u64 {
    0
}

/// Sem relatórios de erro com dump de memória. Sem efeito por enquanto no Linux.
/// No error reports with memory dumps. No effect on Linux yet.
pub fn exclude_from_error_reporting() {}

/// Só existe para o WebView2 do Windows. / Only exists for Windows' WebView2.
pub fn harden_webview_env() {}
