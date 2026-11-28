//! Socket Unix protegido entre o app (servidor) e o host (cliente). É o equivalente, no Linux,
//! do named pipe do Windows.
//! Protected Unix socket between the app (server) and the host (client). It is the Linux
//! counterpart of the Windows named pipe.
//!
//! Proteções / Protections:
//! - O socket fica numa pasta só do usuário (`0700`, dona nossa), e o próprio socket é `0600`.
//!   The socket lives in a user-only folder (`0700`, owned by us), and the socket itself is `0600`.
//! - Se outro programa já escuta nesse nome, o app não aceita ser "segundo" e falha. Um socket
//!   velho, sem ninguém atrás, é removido.
//!   If another program already listens on this name, the app refuses to be "second" and fails.
//!   A stale socket, with nobody behind it, is removed.
//! - Quem conecta precisa ser o mesmo usuário (`SO_PEERCRED`), nos dois sentidos.
//!   Whoever connects must be the same user (`SO_PEERCRED`), in both directions.

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn euid() -> u32 {
    // SAFETY: `geteuid` não recebe argumentos e não falha. / `geteuid` takes no arguments and cannot fail.
    unsafe { libc::geteuid() }
}

/// Pasta do socket: `$XDG_RUNTIME_DIR/sigilvault`, ou `/tmp/sigilvault-<uid>` se não houver.
/// Socket folder: `$XDG_RUNTIME_DIR/sigilvault`, or `/tmp/sigilvault-<uid>` if there is none.
fn base_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|p| p.is_absolute()) {
        Some(run) => run.join("sigilvault"),
        None => PathBuf::from(format!("/tmp/sigilvault-{}", euid())),
    }
}

/// Caminho do socket do usuário atual. / Current user's socket path.
pub fn default_name() -> io::Result<String> {
    Ok(base_dir().join("bridge.sock").to_string_lossy().into_owned())
}

/// Garante que a pasta existe, é nossa, não é link e só nós mexemos nela.
/// Makes sure the folder exists, is ours, is not a link and only we can touch it.
fn private_dir(dir: &Path) -> io::Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let meta = fs::symlink_metadata(dir)?;
    if !meta.is_dir() || meta.uid() != euid() || meta.permissions().mode() & 0o077 != 0 {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "pasta do socket insegura / insecure socket folder"));
    }
    Ok(())
}

/// Lê quem está do outro lado do socket. / Reads who is on the other side of the socket.
fn peer_cred(stream: &UnixStream) -> io::Result<libc::ucred> {
    use std::os::fd::AsRawFd;
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `cred` e `len` são válidos e têm o tamanho que `SO_PEERCRED` espera.
    // SAFETY: `cred` and `len` are valid and have the size `SO_PEERCRED` expects.
    let rc = unsafe { libc::getsockopt(stream.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&raw mut cred).cast(), &mut len) };
    if rc == 0 { Ok(cred) } else { Err(io::Error::last_os_error()) }
}

/// Uma conexão (cliente ou servidor). Ler e escrever ao mesmo tempo, de threads diferentes, é permitido.
/// One connection (client or server). Reading and writing at the same time, from different threads, is allowed.
pub struct Pipe {
    stream: UnixStream,
}

impl Pipe {
    /// Conecta ao socket, tentando de novo até `timeout`. Se não há socket ou ninguém escuta,
    /// devolve `NotFound` (o app não está rodando).
    /// Connects to the socket, retrying until `timeout`. If there is no socket or nobody listens,
    /// returns `NotFound` (the app is not running).
    pub fn connect(name: &str, timeout: Duration) -> io::Result<Self> {
        let start = Instant::now();
        loop {
            match UnixStream::connect(name) {
                Ok(stream) => {
                    let pipe = Self { stream };
                    // O servidor precisa ser do mesmo usuário. / The server must be the same user.
                    if peer_cred(&pipe.stream)?.uid != euid() {
                        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "servidor de outro usuário / server of another user"));
                    }
                    return Ok(pipe);
                }
                // Recusado = socket velho sem dono; para nós é o mesmo que não existir.
                // Refused = stale socket with no owner; for us it is the same as missing.
                Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused) => {
                    return Err(io::Error::new(io::ErrorKind::NotFound, e));
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock && start.elapsed() < timeout => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// PID do processo do outro lado. / PID of the process on the other side.
    pub fn peer_pid(&self) -> io::Result<u32> {
        let cred = peer_cred(&self.stream)?;
        u32::try_from(cred.pid).map_err(|_| io::Error::other("pid inválido / invalid pid"))
    }

    /// Encerra a conexão, acordando quem estiver lendo. / Ends the connection, waking whoever is reading.
    pub fn cancel(&self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

impl Read for &Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.stream).read(buf)
    }
}

impl Write for &Pipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.stream).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.stream).flush()
    }
}

/// O lado que escuta (o app). / The listening side (the app).
pub struct Listener {
    listener: UnixListener,
    path: PathBuf,
}

impl Listener {
    /// Cria o socket. Falha se outro programa já escuta nesse nome. / Creates the socket. Fails if another program already listens on this name.
    pub fn bind(name: &str) -> io::Result<Self> {
        let path = PathBuf::from(name);
        let dir = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "caminho sem pasta / path without folder"))?;
        private_dir(dir)?;
        if fs::symlink_metadata(&path).is_ok() {
            if UnixStream::connect(&path).is_ok() {
                return Err(io::Error::new(io::ErrorKind::AddrInUse, "já há alguém escutando / someone is already listening"));
            }
            fs::remove_file(&path)?;
        }
        let listener = UnixListener::bind(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        Ok(Self { listener, path })
    }

    /// Espera a próxima conexão, ignorando as de outro usuário. / Waits for the next connection, ignoring other users'.
    pub fn accept(&mut self) -> io::Result<Pipe> {
        loop {
            let (stream, _) = self.listener.accept()?;
            let pipe = Pipe { stream };
            if peer_cred(&pipe.stream).is_ok_and(|c| c.uid == euid()) {
                return Ok(pipe);
            }
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn test_name(tag: &str) -> String {
        std::env::temp_dir().join(format!("sigilvault-test-{tag}-{}", std::process::id())).join("bridge.sock").to_string_lossy().into_owned()
    }

    #[test]
    fn roundtrip_and_peer_pid() {
        let name = test_name("rt");
        let mut listener = Listener::bind(&name).unwrap();
        let client = std::thread::spawn({
            let name = name.clone();
            move || {
                let pipe = Pipe::connect(&name, Duration::from_secs(2)).unwrap();
                (&pipe).write_all(b"ola").unwrap();
                let mut buf = [0u8; 3];
                (&pipe).read_exact(&mut buf).unwrap();
                buf
            }
        });
        let server = listener.accept().unwrap();
        assert_eq!(server.peer_pid().unwrap(), std::process::id());
        let mut buf = [0u8; 3];
        (&server).read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ola");
        (&server).write_all(b"oi!").unwrap();
        assert_eq!(&client.join().unwrap(), b"oi!");
        let _ = fs::remove_dir_all(Path::new(&name).parent().unwrap());
    }

    #[test]
    fn second_server_with_same_name_is_refused() {
        let name = test_name("dup");
        let _first = Listener::bind(&name).unwrap();
        assert_eq!(Listener::bind(&name).err().map(|e| e.kind()), Some(io::ErrorKind::AddrInUse));
        let _ = fs::remove_dir_all(Path::new(&name).parent().unwrap());
    }

    #[test]
    fn stale_socket_is_replaced_and_missing_one_is_not_found() {
        let name = test_name("stale");
        assert_eq!(Pipe::connect(&name, Duration::from_millis(100)).err().map(|e| e.kind()), Some(io::ErrorKind::NotFound));
        drop(Listener::bind(&name).unwrap());
        // Sem ninguém escutando, nem existe mais o arquivo. / With nobody listening, the file is gone too.
        assert!(Listener::bind(&name).is_ok());
        let _ = fs::remove_dir_all(Path::new(&name).parent().unwrap());
    }

    #[test]
    fn cancel_unblocks_reader() {
        let name = test_name("cancel");
        let mut listener = Listener::bind(&name).unwrap();
        let client = Pipe::connect(&name, Duration::from_secs(2)).unwrap();
        let server = listener.accept().unwrap();
        let reader = std::thread::scope(|s| {
            let h = s.spawn(|| {
                let mut buf = [0u8; 1];
                (&server).read(&mut buf).unwrap()
            });
            std::thread::sleep(Duration::from_millis(100));
            server.cancel();
            h.join().unwrap()
        });
        assert_eq!(reader, 0);
        drop(client);
        let _ = fs::remove_dir_all(Path::new(&name).parent().unwrap());
    }

    #[test]
    fn insecure_folder_is_refused() {
        let dir = std::env::temp_dir().join(format!("sigilvault-test-open-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        let name = dir.join("bridge.sock").to_string_lossy().into_owned();
        assert_eq!(Listener::bind(&name).err().map(|e| e.kind()), Some(io::ErrorKind::PermissionDenied));
        let _ = fs::remove_dir_all(dir);
    }
}
