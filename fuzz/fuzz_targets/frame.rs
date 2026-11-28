//! Mensagens entre o host e o app (4 bytes de tamanho + conteúdo), com o limite de 64 KiB.
//! Messages between the host and the app (4-byte length + content), with the 64 KiB limit.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(Some(msg)) = sigilvault_ipc::frame::read(data, sigilvault_ipc::MAX_MESSAGE) {
        assert!(msg.len() <= sigilvault_ipc::MAX_MESSAGE);
        // Escrever e ler de volta devolve a mesma mensagem. / Writing and reading back gives the same message.
        let mut out = Vec::new();
        sigilvault_ipc::frame::write(&mut out, &msg, sigilvault_ipc::MAX_MESSAGE).unwrap();
        assert_eq!(sigilvault_ipc::frame::read(&out[..], sigilvault_ipc::MAX_MESSAGE).unwrap().unwrap(), msg);
    }
});
