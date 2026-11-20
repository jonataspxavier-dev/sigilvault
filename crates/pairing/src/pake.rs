//! OPAQUE (RFC 9807) com o código de pareamento como senha.
//! OPAQUE (RFC 9807) with the pairing code as password.
//!
//! O app faz o "cadastro" sozinho (ele mesmo sorteou o código) e guarda o registro só na memória.
//! Depois a extensão faz o "login" com o código digitado. Os dois lados terminam com a mesma
//! chave de sessão, e cada um confere o outro (autenticação mútua).
//!
//! The app does the "registration" by itself (it drew the code) and keeps the record only in
//! memory. Then the extension does the "login" with the typed code. Both sides end up with the
//! same session key, and each one checks the other (mutual authentication).
//!
//! Função de esticamento (KSF) = identidade: o registro nunca é gravado em disco e só aceita
//! poucas tentativas online, então Argon2 não protegeria nada a mais aqui.
//! Key stretching function (KSF) = identity: the record is never written to disk and only
//! accepts a few online attempts, so Argon2 would not add protection here.

use hkdf::Hkdf;
use opaque_ke::ksf::Identity;
use opaque_ke::{
    CipherSuite, ClientLogin, ClientLoginFinishParameters, ClientRegistration, ClientRegistrationFinishParameters,
    CredentialFinalization, CredentialRequest, CredentialResponse, Identifiers, Ristretto255, ServerLoginParameters,
    ServerRegistration, ServerSetup, TripleDh,
};
use rand_core::{CryptoRng, RngCore};
use sha2::{Sha256, Sha512};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::code::PairingCode;
use crate::Error;

/// Conjunto de algoritmos: Ristretto255 + 3DH com SHA-512. / Algorithm suite: Ristretto255 + 3DH with SHA-512.
pub struct Suite;

impl CipherSuite for Suite {
    type OprfCs = Ristretto255;
    type KeyExchange = TripleDh<Ristretto255, Sha512>;
    type Ksf = Identity;
}

/// Contexto e identidades amarrados ao protocolo: uma troca de outro programa não serve aqui.
/// Context and identities bound to the protocol: an exchange from another program does not fit here.
const CONTEXT: &[u8] = b"SigilVault pairing v1";
const CLIENT_ID: &[u8] = b"sigilvault-extension";
const SERVER_ID: &[u8] = b"sigilvault-app";
const CREDENTIAL_ID: &[u8] = b"pairing";
const PAIRING_KEY_INFO: &[u8] = b"SigilVault pairing key v1";

fn identifiers() -> Identifiers<'static> {
    Identifiers { client: Some(CLIENT_ID), server: Some(SERVER_ID) }
}

fn server_params() -> ServerLoginParameters<'static, 'static> {
    ServerLoginParameters { context: Some(CONTEXT), identifiers: identifiers() }
}

/// Chave de sessão da OPAQUE (64 bytes), apagada da memória ao sair de uso.
/// OPAQUE session key (64 bytes), wiped when dropped.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SessionKey([u8; 64]);

impl SessionKey {
    /// Só para testes. / Tests only.
    #[cfg(test)]
    fn same_as(&self, other: &SessionKey) -> bool {
        self.0 == other.0
    }
}

impl From<&[u8]> for SessionKey {
    fn from(bytes: &[u8]) -> Self {
        let mut k = [0u8; 64];
        k.copy_from_slice(&bytes[..64]);
        Self(k)
    }
}

/// Chave permanente do pareamento: HKDF-SHA256(chave de sessão, info = rótulo || id).
/// O id entra na derivação: se alguém trocar o id no caminho, as chaves não batem.
///
/// Permanent pairing key: HKDF-SHA256(session key, info = label || id).
/// The id is part of the derivation: if someone swaps the id on the way, the keys do not match.
pub fn pairing_key(session: &SessionKey, pairing_id: &[u8; 16]) -> [u8; 32] {
    let mut info = [0u8; PAIRING_KEY_INFO.len() + 16];
    info[..PAIRING_KEY_INFO.len()].copy_from_slice(PAIRING_KEY_INFO);
    info[PAIRING_KEY_INFO.len()..].copy_from_slice(pairing_id);
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(None, &session.0)
        .expand(&info, &mut out)
        .expect("32 bytes cabem no HKDF / 32 bytes fit in HKDF");
    out
}

// ── Lado do app (servidor) / App side (server) ────────────────────

/// Pareamento pendente no app: o registro OPAQUE do código mostrado na tela.
/// Pending pairing in the app: the OPAQUE record of the code shown on screen.
pub struct ServerPairing {
    setup: ServerSetup<Suite>,
    record: ServerRegistration<Suite>,
}

impl ServerPairing {
    /// Cadastra o código localmente. / Registers the code locally.
    pub fn new<R: RngCore + CryptoRng>(rng: &mut R, code: &PairingCode) -> Result<Self, Error> {
        let setup = ServerSetup::<Suite>::new(rng);
        let start = ClientRegistration::<Suite>::start(rng, code.as_bytes()).map_err(|_| Error::Rejected)?;
        let response = ServerRegistration::<Suite>::start(&setup, start.message, CREDENTIAL_ID).map_err(|_| Error::Rejected)?;
        let finish = start
            .state
            .finish(rng, code.as_bytes(), response.message, ClientRegistrationFinishParameters::new(identifiers(), None))
            .map_err(|_| Error::Rejected)?;
        Ok(Self { setup, record: ServerRegistration::finish(finish.message) })
    }

    /// Responde ao primeiro passo do login da extensão. Cada chamada é uma tentativa.
    /// Answers the extension's first login step. Each call is one attempt.
    pub fn login_start<R: RngCore + CryptoRng>(&self, rng: &mut R, request: &[u8]) -> Result<(ServerLogin, Vec<u8>), Error> {
        let request = CredentialRequest::<Suite>::deserialize(request).map_err(|_| Error::Rejected)?;
        let start = opaque_ke::ServerLogin::<Suite>::start(rng, &self.setup, Some(self.record.clone()), request, CREDENTIAL_ID, server_params())
            .map_err(|_| Error::Rejected)?;
        Ok((ServerLogin(start.state), start.message.serialize().to_vec()))
    }
}

/// Estado do app entre os dois passos do login. / App state between the two login steps.
pub struct ServerLogin(opaque_ke::ServerLogin<Suite>);

impl ServerLogin {
    /// Confere a prova da extensão. Falha se o código digitado estava errado.
    /// Checks the extension's proof. Fails if the typed code was wrong.
    pub fn finish(self, finalization: &[u8]) -> Result<SessionKey, Error> {
        let fin = CredentialFinalization::<Suite>::deserialize(finalization).map_err(|_| Error::Rejected)?;
        let result = self.0.finish(fin, server_params()).map_err(|_| Error::Rejected)?;
        Ok(SessionKey::from(result.session_key.as_slice()))
    }
}

// ── Lado da extensão (cliente) / Extension side (client) ──────────

/// Login em andamento na extensão. / Login in progress in the extension.
pub struct ClientPairing {
    state: ClientLogin<Suite>,
    code: PairingCode,
}

impl ClientPairing {
    /// Primeiro passo: devolve o estado e a mensagem para o app. / First step: returns the state and the message for the app.
    pub fn start<R: RngCore + CryptoRng>(rng: &mut R, code: PairingCode) -> Result<(Self, Vec<u8>), Error> {
        let start = ClientLogin::<Suite>::start(rng, code.as_bytes()).map_err(|_| Error::Rejected)?;
        Ok((Self { state: start.state, code }, start.message.serialize().to_vec()))
    }

    /// Segundo passo: confere o app (falha se o código estiver errado) e devolve a mensagem final
    /// e a chave de sessão. / Second step: checks the app (fails if the code is wrong) and returns
    /// the final message and the session key.
    pub fn finish<R: RngCore + CryptoRng>(self, rng: &mut R, response: &[u8]) -> Result<(Vec<u8>, SessionKey), Error> {
        let response = CredentialResponse::<Suite>::deserialize(response).map_err(|_| Error::Rejected)?;
        let params = ClientLoginFinishParameters::new(Some(CONTEXT), identifiers(), None);
        let result = self.state.finish(rng, self.code.as_bytes(), response, params).map_err(|_| Error::Rejected)?;
        Ok((result.message.serialize().to_vec(), SessionKey::from(result.session_key.as_slice())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FillRng;

    fn rng() -> FillRng<impl FnMut(&mut [u8])> {
        FillRng(|b: &mut [u8]| getrandom::fill(b).unwrap())
    }

    fn code() -> PairingCode {
        PairingCode::generate(&mut |b: &mut [u8]| getrandom::fill(b).unwrap())
    }

    #[test]
    fn right_code_gives_same_keys_on_both_sides() {
        let code = code();
        let server = ServerPairing::new(&mut rng(), &code).unwrap();
        let typed = PairingCode::parse(&code.to_display_string().to_lowercase()).unwrap();
        let (client, req) = ClientPairing::start(&mut rng(), typed).unwrap();
        let (login, resp) = server.login_start(&mut rng(), &req).unwrap();
        let (fin, client_key) = client.finish(&mut rng(), &resp).unwrap();
        let server_key = login.finish(&fin).unwrap();
        assert!(client_key.same_as(&server_key));
        let id = [7u8; 16];
        assert_eq!(pairing_key(&client_key, &id), pairing_key(&server_key, &id));
        assert_ne!(pairing_key(&client_key, &id), pairing_key(&client_key, &[8u8; 16]));
    }

    #[test]
    fn wrong_code_is_rejected_by_the_client() {
        let server = ServerPairing::new(&mut rng(), &code()).unwrap();
        let (client, req) = ClientPairing::start(&mut rng(), code()).unwrap();
        let (_login, resp) = server.login_start(&mut rng(), &req).unwrap();
        assert_eq!(client.finish(&mut rng(), &resp).err(), Some(Error::Rejected));
    }

    #[test]
    fn forged_finalization_is_rejected_by_the_server() {
        let code = code();
        let server = ServerPairing::new(&mut rng(), &code).unwrap();
        // Um atacante com o código errado tenta terminar mesmo assim com uma prova inventada.
        // An attacker with the wrong code tries to finish anyway with a made-up proof.
        let (_client, req) = ClientPairing::start(&mut rng(), self::code()).unwrap();
        let (login, _resp) = server.login_start(&mut rng(), &req).unwrap();
        assert_eq!(login.finish(&[0u8; 64]).err(), Some(Error::Rejected));
        // E a prova de outra sessão (mesmo com o código certo) também não vale.
        // And the proof from another session (even with the right code) does not work either.
        let (client, req) = ClientPairing::start(&mut rng(), code.clone()).unwrap();
        let (_login1, resp) = server.login_start(&mut rng(), &req).unwrap();
        let (fin, _) = client.finish(&mut rng(), &resp).unwrap();
        let (client2, req2) = ClientPairing::start(&mut rng(), code).unwrap();
        let (login2, _resp2) = server.login_start(&mut rng(), &req2).unwrap();
        drop(client2);
        assert_eq!(login2.finish(&fin).err(), Some(Error::Rejected));
    }

    #[test]
    fn tampered_messages_are_rejected() {
        let code = code();
        let server = ServerPairing::new(&mut rng(), &code).unwrap();
        let (client, req) = ClientPairing::start(&mut rng(), code).unwrap();
        let (_login, mut resp) = server.login_start(&mut rng(), &req).unwrap();
        let last = resp.len() - 1;
        resp[last] ^= 1;
        assert_eq!(client.finish(&mut rng(), &resp).err(), Some(Error::Rejected));
        assert!(server.login_start(&mut rng(), &[1, 2, 3]).is_err());
    }
}
