//! Chave de 32 bytes guardada em memória travada.
//!
//! Quando falta memória, o sistema pode copiar páginas de um programa para o disco (arquivo de
//! paginação ou swap), sem criptografia. Esta crate reserva uma página só para cada chave
//! (`VirtualAlloc` no Windows, `mmap` no Linux) e pede ao sistema que a mantenha na RAM
//! (`VirtualLock`, `mlock`). Uma página por chave porque o `VirtualLock` não conta referências:
//! destravar uma chave destravaria a vizinha da mesma página. No Linux a página ainda fica fora
//! dos despejos de memória (`MADV_DONTDUMP`) e não passa para processos filhos
//! (`MADV_WIPEONFORK`). Ao descartar, a chave é apagada, a página destravada e devolvida.
//!
//! Limites: hibernação e despejos de memória (crash dumps) do Windows ainda gravam a RAM inteira;
//! o travamento é "melhor esforço" — se o sistema recusar (no Linux, o limite `RLIMIT_MEMLOCK`),
//! a chave continua protegida por zeroize, só sem a trava.
//!
//! 32-byte key kept in locked memory.
//!
//! When memory runs low, the system can copy a program's pages to disk (page file or swap),
//! unencrypted. This crate reserves a page of its own for each key (`VirtualAlloc` on Windows,
//! `mmap` on Linux) and asks the system to keep it in RAM (`VirtualLock`, `mlock`). One page per
//! key because `VirtualLock` does not count references: unlocking one key would unlock its
//! neighbor on the same page. On Linux the page also stays out of memory dumps (`MADV_DONTDUMP`)
//! and is not passed to child processes (`MADV_WIPEONFORK`). On drop the key is wiped, the page
//! unlocked and released.
//!
//! Limits: Windows hibernation and crash dumps still write all of RAM; locking is "best effort" —
//! if the system refuses (on Linux, the `RLIMIT_MEMLOCK` limit), the key is still protected by
//! zeroize, just without the lock.

use std::fmt;
use std::ops::{Deref, DerefMut};

use zeroize::Zeroize;

/// Tamanho da chave (256 bits). / Key size (256 bits).
pub const KEY_LEN: usize = 32;

/// Chave em memória travada. Endereço fixo: mover o `LockedKey` não move os bytes.
/// Key in locked memory. Fixed address: moving the `LockedKey` does not move the bytes.
pub struct LockedKey {
    mem: imp::Page,
}

impl LockedKey {
    /// Copia `key` para uma página travada. Quem chama continua responsável por apagar a origem
    /// (use `Zeroizing`). / Copies `key` into a locked page. The caller remains responsible for
    /// wiping the source (use `Zeroizing`).
    pub fn new(key: &[u8; KEY_LEN]) -> Self {
        let mut mem = imp::Page::new();
        mem.bytes_mut().copy_from_slice(key);
        Self { mem }
    }

    /// A página está de fato travada na RAM? / Is the page actually locked in RAM?
    pub fn is_locked(&self) -> bool {
        self.mem.locked()
    }
}

impl Deref for LockedKey {
    type Target = [u8; KEY_LEN];
    fn deref(&self) -> &[u8; KEY_LEN] {
        self.mem.bytes()
    }
}

impl DerefMut for LockedKey {
    fn deref_mut(&mut self) -> &mut [u8; KEY_LEN] {
        self.mem.bytes_mut()
    }
}

impl Clone for LockedKey {
    fn clone(&self) -> Self {
        Self::new(self)
    }
}

impl Zeroize for LockedKey {
    fn zeroize(&mut self) {
        self.mem.bytes_mut().zeroize();
    }
}

impl Drop for LockedKey {
    fn drop(&mut self) {
        // A página é liberada pelo Drop de `Page`, depois de apagada aqui.
        // The page is released by `Page`'s Drop, after being wiped here.
        self.zeroize();
    }
}

impl fmt::Debug for LockedKey {
    /// Nunca mostra a chave. / Never shows the key.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LockedKey").field("locked", &self.is_locked()).finish_non_exhaustive()
    }
}

#[cfg(windows)]
mod imp {
    use super::KEY_LEN;
    use windows_sys::Win32::System::Memory::{MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc, VirtualFree, VirtualLock, VirtualUnlock};

    /// Uma página do sistema só para a chave, ou um `Box` se o `VirtualAlloc` falhar.
    /// A system page just for the key, or a `Box` if `VirtualAlloc` fails.
    pub(super) enum Page {
        Virtual { ptr: *mut [u8; KEY_LEN], locked: bool },
        Fallback(Box<[u8; KEY_LEN]>),
    }

    // SAFETY: a página pertence só a este valor; o acesso segue as regras de & / &mut.
    // SAFETY: the page belongs only to this value; access follows & / &mut rules.
    unsafe impl Send for Page {}
    unsafe impl Sync for Page {}

    impl Page {
        pub(super) fn new() -> Self {
            // SAFETY: pedimos uma página nova (zerada pelo Windows); só usamos os primeiros 32 bytes.
            // SAFETY: we request a new page (zeroed by Windows); we only use the first 32 bytes.
            unsafe {
                let ptr = VirtualAlloc(std::ptr::null(), KEY_LEN, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
                if ptr.is_null() {
                    return Page::Fallback(Box::new([0; KEY_LEN]));
                }
                let locked = VirtualLock(ptr, KEY_LEN) != 0;
                Page::Virtual { ptr: ptr.cast(), locked }
            }
        }

        pub(super) fn locked(&self) -> bool {
            matches!(self, Page::Virtual { locked: true, .. })
        }

        pub(super) fn bytes(&self) -> &[u8; KEY_LEN] {
            match self {
                // SAFETY: ponteiro válido até o Drop; alinhado (início de página).
                // SAFETY: pointer valid until Drop; aligned (start of a page).
                Page::Virtual { ptr, .. } => unsafe { &**ptr },
                Page::Fallback(b) => b,
            }
        }

        pub(super) fn bytes_mut(&mut self) -> &mut [u8; KEY_LEN] {
            match self {
                // SAFETY: idem, e `&mut self` garante acesso exclusivo. / SAFETY: same, and `&mut self` ensures exclusive access.
                Page::Virtual { ptr, .. } => unsafe { &mut **ptr },
                Page::Fallback(b) => b,
            }
        }
    }

    impl Drop for Page {
        fn drop(&mut self) {
            if let Page::Virtual { ptr, locked } = *self {
                // SAFETY: a página foi apagada pelo LockedKey; destrava e devolve ao Windows.
                // SAFETY: the page was wiped by LockedKey; unlock it and give it back to Windows.
                unsafe {
                    if locked {
                        VirtualUnlock(ptr.cast(), KEY_LEN);
                    }
                    VirtualFree(ptr.cast(), 0, MEM_RELEASE);
                }
            }
        }
    }
}

#[cfg(unix)]
mod imp {
    use super::KEY_LEN;

    /// Uma página anônima só para a chave, ou um `Box` se o `mmap` falhar.
    /// An anonymous page just for the key, or a `Box` if `mmap` fails.
    pub(super) enum Page {
        Mapped { ptr: *mut [u8; KEY_LEN], len: usize, locked: bool },
        Fallback(Box<[u8; KEY_LEN]>),
    }

    // SAFETY: a página pertence só a este valor; o acesso segue as regras de & / &mut.
    // SAFETY: the page belongs only to this value; access follows & / &mut rules.
    unsafe impl Send for Page {}
    unsafe impl Sync for Page {}

    fn page_size() -> usize {
        // SAFETY: `sysconf` só consulta um valor do sistema. / `sysconf` only queries a system value.
        let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        usize::try_from(size).ok().filter(|s| *s >= KEY_LEN).unwrap_or(4096)
    }

    impl Page {
        pub(super) fn new() -> Self {
            let len = page_size();
            // SAFETY: pedimos uma página anônima nova (zerada pelo sistema); só usamos os primeiros
            // 32 bytes. O resultado é conferido antes de qualquer uso.
            // SAFETY: we request a new anonymous page (zeroed by the system); we only use the first
            // 32 bytes. The result is checked before any use.
            unsafe {
                let ptr = libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_PRIVATE | libc::MAP_ANONYMOUS, -1, 0);
                if ptr == libc::MAP_FAILED {
                    return Page::Fallback(Box::new([0; KEY_LEN]));
                }
                let locked = libc::mlock(ptr, len) == 0;
                // Fora de despejos de memória e dos processos filhos. Se o sistema não aceitar
                // (núcleo antigo), a chave continua protegida pelo resto.
                // Out of memory dumps and child processes. If the system does not accept it (old
                // kernel), the key is still protected by the rest.
                #[cfg(target_os = "linux")]
                {
                    libc::madvise(ptr, len, libc::MADV_DONTDUMP);
                    libc::madvise(ptr, len, libc::MADV_WIPEONFORK);
                }
                Page::Mapped { ptr: ptr.cast(), len, locked }
            }
        }

        pub(super) fn locked(&self) -> bool {
            matches!(self, Page::Mapped { locked: true, .. })
        }

        pub(super) fn bytes(&self) -> &[u8; KEY_LEN] {
            match self {
                // SAFETY: ponteiro válido até o Drop; alinhado (início de página).
                // SAFETY: pointer valid until Drop; aligned (start of a page).
                Page::Mapped { ptr, .. } => unsafe { &**ptr },
                Page::Fallback(b) => b,
            }
        }

        pub(super) fn bytes_mut(&mut self) -> &mut [u8; KEY_LEN] {
            match self {
                // SAFETY: idem, e `&mut self` garante acesso exclusivo. / SAFETY: same, and `&mut self` ensures exclusive access.
                Page::Mapped { ptr, .. } => unsafe { &mut **ptr },
                Page::Fallback(b) => b,
            }
        }
    }

    impl Drop for Page {
        fn drop(&mut self) {
            if let Page::Mapped { ptr, len, locked } = *self {
                // SAFETY: a página foi apagada pelo LockedKey; destrava e devolve ao sistema.
                // SAFETY: the page was wiped by LockedKey; unlock it and give it back to the system.
                unsafe {
                    if locked {
                        libc::munlock(ptr.cast(), len);
                    }
                    libc::munmap(ptr.cast(), len);
                }
            }
        }
    }
}

#[cfg(not(any(windows, unix)))]
mod imp {
    use super::KEY_LEN;

    /// Em outros sistemas: só um `Box` (sem trava). / On other systems: just a `Box` (no lock).
    pub(super) struct Page(Box<[u8; KEY_LEN]>);

    impl Page {
        pub(super) fn new() -> Self {
            Page(Box::new([0; KEY_LEN]))
        }
        pub(super) fn locked(&self) -> bool {
            false
        }
        pub(super) fn bytes(&self) -> &[u8; KEY_LEN] {
            &self.0
        }
        pub(super) fn bytes_mut(&mut self) -> &mut [u8; KEY_LEN] {
            &mut self.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No Linux, travar depende do limite `RLIMIT_MEMLOCK` (em alguns contêineres é zero): só
    /// exigimos a trava quando o limite deixa. / On Linux, locking depends on the `RLIMIT_MEMLOCK`
    /// limit (zero in some containers): we only require the lock when the limit allows it.
    fn lock_expected() -> bool {
        #[cfg(windows)]
        {
            true
        }
        #[cfg(unix)]
        {
            let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
            // SAFETY: `limit` é válido para escrita. / `limit` is valid for writing.
            let ok = unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, &mut limit) } == 0;
            ok && limit.rlim_cur >= 1024 * 1024
        }
        #[cfg(not(any(windows, unix)))]
        {
            false
        }
    }

    #[test]
    fn keeps_the_bytes_and_locks_the_page() {
        let key = [7u8; KEY_LEN];
        let locked = LockedKey::new(&key);
        assert_eq!(*locked, key);
        if lock_expected() {
            assert!(locked.is_locked(), "o sistema deveria aceitar travar uma página / the system should accept locking one page");
        }
        let copy = locked.clone();
        assert_eq!(*copy, key);
        // Endereços diferentes: cada chave na sua página. / Different addresses: each key on its own page.
        assert_ne!(locked.as_ptr(), copy.as_ptr());
    }

    #[test]
    fn address_is_stable_when_moved() {
        let a = LockedKey::new(&[1; KEY_LEN]);
        let before = a.as_ptr();
        let moved = Box::new(a);
        assert_eq!(moved.as_ptr(), before);
    }

    #[test]
    fn zeroize_clears() {
        let mut k = LockedKey::new(&[9; KEY_LEN]);
        k.zeroize();
        assert_eq!(*k, [0; KEY_LEN]);
    }

    #[test]
    fn many_keys_fit() {
        // Abre/fecha várias vezes o cofre ao longo do dia: nada pode vazar páginas travadas. 5000
        // páginas passam do limite padrão do Linux (8 MiB), então um vazamento seria notado aqui.
        // Opening/closing the vault many times a day: no locked pages may leak. 5000 pages exceed
        // Linux's default limit (8 MiB), so a leak would be noticed here.
        for _ in 0..5000 {
            let k = LockedKey::new(&[3; KEY_LEN]);
            if lock_expected() {
                assert!(k.is_locked());
            }
        }
    }
}
