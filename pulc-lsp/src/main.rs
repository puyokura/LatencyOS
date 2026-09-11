//! pulc-lsp - Dedicated standalone PulseLang Language Server Protocol (LSP) daemon
//!
//! Provides JSON-RPC 2.0 communication over stdio for VSCode, Zed, Neovim, etc.

use pulselang_core::lsp::LspServer;
use std::io::{stdin, stdout, BufReader};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut server = LspServer::new();
    let stdin = stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout().lock();

    while let Ok(Some(msg)) = LspServer::read_message(&mut reader) {
        if let Some(resp) = server.handle_request(&msg) {
            if LspServer::write_message(&mut writer, &resp).is_err() {
                return ExitCode::from(2);
            }
        }
    }

    ExitCode::SUCCESS
}
