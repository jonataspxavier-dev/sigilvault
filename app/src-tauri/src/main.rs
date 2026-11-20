//! Ponto de entrada. Em release, não abre janela de console.
//! Entry point. In release, no console window is opened.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    sigilvault_app_lib::run();
}
