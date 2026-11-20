//! Named pipe protegido entre o app (servidor) e o host (cliente).
//! Protected named pipe between the app (server) and the host (client).
//!
//! Proteções / Protections:
//! - DACL explícita: só o usuário atual pode abrir ou criar instâncias.
//!   Explicit DACL: only the current user can open or create instances.
//! - `FILE_FLAG_FIRST_PIPE_INSTANCE`: se outro programa já criou um pipe com esse nome, o app
//!   não aceita ser "segundo" e falha. Sempre há uma instância nossa ouvindo, então o nome nunca
//!   fica livre. / If another program already created a pipe with this name, the app refuses to
//!   be "second" and fails. There is always one of our instances listening, so the name is never free.
//! - `PIPE_REJECT_REMOTE_CLIENTS`: nada pela rede. / Nothing over the network.
//! - O cliente abre com `SECURITY_IDENTIFICATION`: o servidor não consegue se passar por ele.
//!   The client opens with `SECURITY_IDENTIFICATION`: the server cannot impersonate it.
//! - I/O "overlapped": ler e escrever ao mesmo tempo na mesma conexão (no modo comum, uma leitura
//!   parada bloquearia as escritas). / Overlapped I/O: read and write at the same time on the same
//!   connection (in normal mode, a pending read would block writes).

use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, GetLastError,
    HANDLE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile,
    SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
};
use windows_sys::Win32::System::Threading::CreateEventW;

use crate::user::current_sid;

/// Nome do pipe do usuário atual: `\\.\pipe\SigilVault-<SID>`. / Current user's pipe name.
pub fn default_name() -> io::Result<String> {
    Ok(format!(r"\\.\pipe\SigilVault-{}", current_sid()?))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Descritor de segurança "só o usuário atual, acesso total". / "Current user only, full access" descriptor.
struct OwnerOnly(PSECURITY_DESCRIPTOR);

impl OwnerOnly {
    fn new() -> io::Result<Self> {
        // D:P = DACL protegida (não herda nada); uma única regra: permitir tudo ao nosso SID.
        // D:P = protected DACL (inherits nothing); a single rule: allow everything to our SID.
        let sddl = wide(&format!("D:P(A;;GA;;;{})", current_sid()?));
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: string terminada em zero; o descritor é liberado no Drop.
        // SAFETY: zero-terminated string; the descriptor is freed on Drop.
        if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, std::ptr::null_mut()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(sd))
    }
}

impl Drop for OwnerOnly {
    fn drop(&mut self) {
        // SAFETY: memória alocada pelo Windows com LocalAlloc. / Memory allocated by Windows with LocalAlloc.
        unsafe { LocalFree(self.0) };
    }
}

/// Uma conexão do pipe (dos dois lados). / One pipe connection (either side).
pub struct Pipe {
    handle: HANDLE,
    server: bool,
}

// SAFETY: o handle é usado com I/O overlapped, que o Windows permite de várias threads.
// SAFETY: the handle is used with overlapped I/O, which Windows allows from several threads.
unsafe impl Send for Pipe {}
unsafe impl Sync for Pipe {}

impl Drop for Pipe {
    fn drop(&mut self) {
        // SAFETY: handle nosso, fechado uma vez. / Our handle, closed once.
        unsafe { CloseHandle(self.handle) };
    }
}

/// Faz uma operação overlapped e espera o resultado. / Runs an overlapped operation and waits for it.
fn overlapped(handle: HANDLE, op: impl FnOnce(*mut OVERLAPPED) -> i32) -> io::Result<u32> {
    // SAFETY: o OVERLAPPED e o evento vivem até o fim da espera; o evento é fechado no fim.
    // SAFETY: the OVERLAPPED and the event live until the wait ends; the event is closed at the end.
    unsafe {
        let event = CreateEventW(std::ptr::null(), 1, 0, std::ptr::null());
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut ov: OVERLAPPED = std::mem::zeroed();
        ov.hEvent = event;
        let started = op(&mut ov);
        let result = if started == 0 && GetLastError() != ERROR_IO_PENDING {
            Err(io::Error::last_os_error())
        } else {
            let mut n = 0u32;
            if GetOverlappedResult(handle, &ov, &mut n, 1) == 0 { Err(io::Error::last_os_error()) } else { Ok(n) }
        };
        CloseHandle(event);
        result
    }
}

impl Pipe {
    /// Conecta como cliente, esperando até `timeout` se o servidor estiver ocupado.
    /// Connects as client, waiting up to `timeout` if the server is busy.
    pub fn connect(name: &str, timeout: Duration) -> io::Result<Self> {
        let name = wide(name);
        let deadline = Instant::now() + timeout;
        loop {
            // SAFETY: nome terminado em zero; sem atributos de segurança (padrão do processo).
            // SAFETY: zero-terminated name; no security attributes (process default).
            let handle = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    std::ptr::null_mut(),
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                return Ok(Self { handle, server: false });
            }
            // SAFETY: leitura do último erro da thread. / Reads the thread's last error.
            let err = unsafe { GetLastError() };
            if err != ERROR_PIPE_BUSY || Instant::now() >= deadline {
                return Err(io::Error::from_raw_os_error(err as i32));
            }
            // SAFETY: nome terminado em zero. / Zero-terminated name.
            unsafe { WaitNamedPipeW(name.as_ptr(), 500) };
        }
    }

    /// PID do processo do outro lado. / PID of the process on the other side.
    pub fn peer_pid(&self) -> io::Result<u32> {
        let mut pid = 0u32;
        // SAFETY: handle válido de pipe. / Valid pipe handle.
        let ok = unsafe {
            if self.server { GetNamedPipeClientProcessId(self.handle, &mut pid) } else { GetNamedPipeServerProcessId(self.handle, &mut pid) }
        };
        if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(pid) }
    }

    /// Cancela leituras e escritas pendentes (de qualquer thread): usado para encerrar a conexão.
    /// Cancels pending reads and writes (from any thread): used to end the connection.
    pub fn cancel(&self) {
        // SAFETY: handle válido. / Valid handle.
        unsafe { CancelIoEx(self.handle, std::ptr::null()) };
    }
}

impl Read for &Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len().min(u32::MAX as usize) as u32;
        let ptr = buf.as_mut_ptr();
        // SAFETY: o buffer vive até a operação terminar (overlapped espera). / The buffer outlives the operation.
        match overlapped(self.handle, |ov| unsafe { ReadFile(self.handle, ptr, len, std::ptr::null_mut(), ov) }) {
            Ok(n) => Ok(n as usize),
            // Pipe fechado do outro lado = fim. / Pipe closed on the other side = end.
            Err(e) if e.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) => Ok(0),
            Err(e) => Err(e),
        }
    }
}

impl Write for &Pipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = buf.len().min(u32::MAX as usize) as u32;
        let ptr = buf.as_ptr();
        // SAFETY: o buffer vive até a operação terminar. / The buffer outlives the operation.
        overlapped(self.handle, |ov| unsafe { WriteFile(self.handle, ptr, len, std::ptr::null_mut(), ov) }).map(|n| n as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Servidor: sempre mantém uma instância ouvindo. / Server: always keeps one instance listening.
pub struct Listener {
    name: Vec<u16>,
    security: OwnerOnly,
    listening: Pipe,
}

impl Listener {
    /// Cria a primeira instância. Falha se o nome já existir (alguém chegou antes).
    /// Creates the first instance. Fails if the name already exists (someone got there first).
    pub fn bind(name: &str) -> io::Result<Self> {
        let name = wide(name);
        let security = OwnerOnly::new()?;
        let listening = Self::instance(&name, &security, true)?;
        Ok(Self { name, security, listening })
    }

    fn instance(name: &[u16], security: &OwnerOnly, first: bool) -> io::Result<Pipe> {
        let attrs = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security.0,
            bInheritHandle: 0,
        };
        let flags = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
        // SAFETY: nome terminado em zero e atributos válidos durante a chamada.
        // SAFETY: zero-terminated name and valid attributes during the call.
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                flags,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                &attrs,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(Pipe { handle, server: true })
    }

    /// Espera o próximo cliente. A nova instância é criada ANTES de entregar a conectada, para o
    /// nome nunca ficar livre. / Waits for the next client. The new instance is created BEFORE
    /// handing over the connected one, so the name is never free.
    pub fn accept(&mut self) -> io::Result<Pipe> {
        let handle = self.listening.handle;
        // SAFETY: handle de servidor válido. / Valid server handle.
        match overlapped(handle, |ov| unsafe { ConnectNamedPipe(handle, ov) }) {
            Ok(_) => {}
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) => {}
            Err(e) => return Err(e),
        }
        let next = Self::instance(&self.name, &self.security, false)?;
        Ok(std::mem::replace(&mut self.listening, next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame;

    fn test_name(tag: &str) -> String {
        format!(r"\\.\pipe\SigilVault-test-{tag}-{}", std::process::id())
    }

    #[test]
    fn roundtrip_and_peer_pid() {
        let name = test_name("rt");
        let mut listener = Listener::bind(&name).unwrap();
        let client_name = name.clone();
        let client = std::thread::spawn(move || {
            let pipe = Pipe::connect(&client_name, Duration::from_secs(5)).unwrap();
            frame::write(&pipe, b"ping", 100).unwrap();
            let reply = frame::read(&pipe, 100).unwrap().unwrap();
            (reply, pipe.peer_pid().unwrap())
        });
        let conn = listener.accept().unwrap();
        assert_eq!(conn.peer_pid().unwrap(), std::process::id());
        assert_eq!(frame::read(&conn, 100).unwrap().unwrap(), b"ping");
        frame::write(&conn, b"pong", 100).unwrap();
        let (reply, server_pid) = client.join().unwrap();
        assert_eq!(reply, b"pong");
        assert_eq!(server_pid, std::process::id());
    }

    #[test]
    fn second_server_with_same_name_is_refused() {
        let name = test_name("dup");
        let _first = Listener::bind(&name).unwrap();
        assert!(Listener::bind(&name).is_err());
    }

    #[test]
    fn write_while_read_is_pending() {
        // Uma thread fica parada lendo enquanto outra escreve na mesma conexão.
        // One thread is blocked reading while another writes on the same connection.
        let name = test_name("dup2");
        let mut listener = Listener::bind(&name).unwrap();
        let client_name = name.clone();
        let client = std::thread::spawn(move || {
            let pipe = Pipe::connect(&client_name, Duration::from_secs(5)).unwrap();
            let got = frame::read(&pipe, 100).unwrap().unwrap();
            frame::write(&pipe, b"bye", 100).unwrap();
            got
        });
        let conn = std::sync::Arc::new(listener.accept().unwrap());
        let reader = {
            let conn = conn.clone();
            std::thread::spawn(move || frame::read(&*conn, 100).unwrap().unwrap())
        };
        std::thread::sleep(Duration::from_millis(200));
        frame::write(&*conn, b"event", 100).unwrap();
        assert_eq!(client.join().unwrap(), b"event");
        assert_eq!(reader.join().unwrap(), b"bye");
    }

    #[test]
    fn cancel_unblocks_reader() {
        let name = test_name("cancel");
        let mut listener = Listener::bind(&name).unwrap();
        let client_name = name.clone();
        let _client = std::thread::spawn(move || {
            let pipe = Pipe::connect(&client_name, Duration::from_secs(5)).unwrap();
            std::thread::sleep(Duration::from_secs(3));
            drop(pipe);
        });
        let conn = std::sync::Arc::new(listener.accept().unwrap());
        let reader = {
            let conn = conn.clone();
            std::thread::spawn(move || frame::read(&*conn, 100))
        };
        std::thread::sleep(Duration::from_millis(200));
        let t = Instant::now();
        conn.cancel();
        assert!(reader.join().unwrap().is_err());
        assert!(t.elapsed() < Duration::from_secs(2));
    }
}
