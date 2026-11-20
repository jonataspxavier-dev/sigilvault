//! Interface WebAssembly para a extensão (lado cliente). Sem wasm-bindgen: poucas funções com
//! ponteiros e tamanhos, e uma única função importada do navegador para o acaso.
//! WebAssembly interface for the extension (client side). No wasm-bindgen: a few functions with
//! pointers and lengths, and a single function imported from the browser for randomness.
//!
//! Uso pela extensão / Use by the extension (extension/src/pake.ts):
//! 1. `sv_alloc` + copiar o código digitado → `sv_login_start` → saída = pedido para o app.
//!    `sv_alloc` + copy the typed code → `sv_login_start` → output = request for the app.
//! 2. resposta do app → `sv_login_finish` → saída = mensagem final (a chave de sessão fica aqui).
//!    app response → `sv_login_finish` → output = final message (the session key stays here).
//! 3. id do pareamento → `sv_pairing_key` → saída = chave de pareamento (32 bytes); o estado é apagado.
//!    pairing id → `sv_pairing_key` → output = pairing key (32 bytes); the state is wiped.
//!
//! Retorno: 0 = ok, 1 = recusado (código errado/mensagem inválida), 2 = código malformado,
//! 3 = fora de ordem. / Return: 0 = ok, 1 = rejected, 2 = malformed code, 3 = out of order.

use std::cell::RefCell;

use zeroize::Zeroize;

use crate::{ClientPairing, Error, FillRng, PairingCode, SessionKey, pairing_key};

#[link(wasm_import_module = "sigilvault")]
unsafe extern "C" {
    /// Preenche `len` bytes com crypto.getRandomValues. / Fills `len` bytes with crypto.getRandomValues.
    fn random_fill(ptr: *mut u8, len: usize);
}

fn rng() -> FillRng<impl FnMut(&mut [u8])> {
    // SAFETY: o navegador escreve exatamente `len` bytes dentro do slice válido.
    // SAFETY: the browser writes exactly `len` bytes inside the valid slice.
    FillRng(|b: &mut [u8]| unsafe { random_fill(b.as_mut_ptr(), b.len()) })
}

#[derive(Default)]
struct State {
    login: Option<ClientPairing>,
    session: Option<SessionKey>,
    out: Vec<u8>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

const OK: i32 = 0;
const REJECTED: i32 = 1;
const MALFORMED: i32 = 2;
const OUT_OF_ORDER: i32 = 3;

/// SAFETY: o chamador passa um ponteiro devolvido por `sv_alloc` com o mesmo tamanho.
/// SAFETY: the caller passes a pointer returned by `sv_alloc` with the same length.
unsafe fn input<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 { &[] } else { unsafe { core::slice::from_raw_parts(ptr, len) } }
}

fn set_out(state: &mut State, data: &[u8]) {
    state.out.zeroize();
    state.out = data.to_vec();
}

/// Reserva memória para a extensão escrever a entrada. / Reserves memory for the extension's input.
#[unsafe(no_mangle)]
pub extern "C" fn sv_alloc(len: usize) -> *mut u8 {
    let mut v = vec![0u8; len.max(1)];
    let p = v.as_mut_ptr();
    core::mem::forget(v);
    p
}

/// Apaga e libera memória de `sv_alloc`. / Wipes and frees memory from `sv_alloc`.
///
/// # Safety
/// `ptr` e `len` precisam vir de `sv_alloc`. / `ptr` and `len` must come from `sv_alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sv_free(ptr: *mut u8, len: usize) {
    let len = len.max(1);
    let mut v = unsafe { Vec::from_raw_parts(ptr, len, len) };
    v.zeroize();
}

/// Ponteiro da última saída. / Pointer to the last output.
#[unsafe(no_mangle)]
pub extern "C" fn sv_out_ptr() -> *const u8 {
    STATE.with_borrow(|s| s.out.as_ptr())
}

/// Tamanho da última saída. / Length of the last output.
#[unsafe(no_mangle)]
pub extern "C" fn sv_out_len() -> usize {
    STATE.with_borrow(|s| s.out.len())
}

/// Apaga todo o estado (login, chave de sessão e saída). / Wipes all state.
#[unsafe(no_mangle)]
pub extern "C" fn sv_reset() {
    STATE.with_borrow_mut(|s| {
        s.login = None;
        s.session = None;
        s.out.zeroize();
    });
}

/// Passo 1: o código digitado (UTF-8). / Step 1: the typed code (UTF-8).
///
/// # Safety
/// Entrada vinda de `sv_alloc`. / Input from `sv_alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sv_login_start(code_ptr: *const u8, code_len: usize) -> i32 {
    sv_reset();
    let Ok(text) = core::str::from_utf8(unsafe { input(code_ptr, code_len) }) else { return MALFORMED };
    let code = match PairingCode::parse(text) {
        Ok(c) => c,
        Err(_) => return MALFORMED,
    };
    match ClientPairing::start(&mut rng(), code) {
        Ok((login, request)) => STATE.with_borrow_mut(|s| {
            s.login = Some(login);
            set_out(s, &request);
            OK
        }),
        Err(_) => REJECTED,
    }
}

/// Passo 2: a resposta do app. / Step 2: the app's response.
///
/// # Safety
/// Entrada vinda de `sv_alloc`. / Input from `sv_alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sv_login_finish(resp_ptr: *const u8, resp_len: usize) -> i32 {
    let response = unsafe { input(resp_ptr, resp_len) };
    STATE.with_borrow_mut(|s| {
        let Some(login) = s.login.take() else { return OUT_OF_ORDER };
        match login.finish(&mut rng(), response) {
            Ok((finalization, key)) => {
                s.session = Some(key);
                set_out(s, &finalization);
                OK
            }
            Err(Error::MalformedCode) => MALFORMED,
            Err(Error::Rejected) => REJECTED,
        }
    })
}

/// Passo 3: o id do pareamento (16 bytes) → chave de pareamento. / Step 3: pairing id → pairing key.
///
/// # Safety
/// Entrada vinda de `sv_alloc`. / Input from `sv_alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sv_pairing_key(id_ptr: *const u8, id_len: usize) -> i32 {
    let Ok(id) = <[u8; 16]>::try_from(unsafe { input(id_ptr, id_len) }) else { return MALFORMED };
    STATE.with_borrow_mut(|s| {
        let Some(session) = s.session.take() else { return OUT_OF_ORDER };
        let mut key = pairing_key(&session, &id);
        set_out(s, &key);
        key.zeroize();
        OK
    })
}
