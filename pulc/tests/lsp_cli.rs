use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn send_rpc(writer: &mut impl Write, body: &str) {
    let msg = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    writer.write_all(msg.as_bytes()).unwrap();
    writer.flush().unwrap();
}

fn read_rpc(reader: &mut impl BufRead) -> Option<String> {
    let mut content_length: Option<usize> = None;
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).ok()?;
        if n == 0 {
            return None;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            if let Ok(len) = rest.trim().parse::<usize>() {
                content_length = Some(len);
            }
        }
    }

    let len = content_length?;
    let mut buf = vec![0u8; len];
    std::io::Read::read_exact(reader, &mut buf).ok()?;
    String::from_utf8(buf).ok()
}

#[test]
fn test_lsp_server_lifecycle_and_diagnostics() {
    let exe = env!("CARGO_BIN_EXE_pulc");

    let mut child = Command::new(exe)
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to spawn pulc lsp");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let mut reader = BufReader::new(stdout);

    // 1. Initialize
    send_rpc(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#,
    );
    let resp = read_rpc(&mut reader).expect("Expected initialize response");
    assert!(resp.contains(r#""id":1"#));
    assert!(resp.contains("capabilities"));
    assert!(resp.contains("hoverProvider"));
    assert!(resp.contains("completionProvider"));

    // 2. Open document with diagnostic error
    send_rpc(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///test.pul","languageId":"pulselang","version":1,"text":"$x := := 10;\n"}}}"#,
    );
    let diag = read_rpc(&mut reader).expect("Expected diagnostics notification");
    assert!(diag.contains("textDocument/publishDiagnostics"));
    assert!(diag.contains("ERR_"));

    // 3. Hover query
    send_rpc(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///test.pul"},"position":{"line":0,"character":1}}}"#,
    );
    let hover_resp = read_rpc(&mut reader).expect("Expected hover response");
    assert!(hover_resp.contains(r#""id":2"#));
    assert!(hover_resp.contains("Variable"));

    // 4. Completion query
    send_rpc(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{"textDocument":{"uri":"file:///test.pul"},"position":{"line":0,"character":0}}}"#,
    );
    let comp_resp = read_rpc(&mut reader).expect("Expected completion response");
    assert!(comp_resp.contains(r#""id":3"#));
    assert!(comp_resp.contains("@contract"));
    assert!(comp_resp.contains("@tsc"));

    // 5. Semantic Tokens query (Editor Syntax Highlighting via LSP)
    send_rpc(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":4,"method":"textDocument/semanticTokens/full","params":{"textDocument":{"uri":"file:///test.pul"}}}"#,
    );
    let sem_resp = read_rpc(&mut reader).expect("Expected semantic tokens response");
    assert!(sem_resp.contains(r#""id":4"#));
    assert!(sem_resp.contains(r#""data":["#));
    // 5. Shutdown and Exit
    send_rpc(&mut stdin, r#"{"jsonrpc":"2.0","id":5,"method":"shutdown","params":null}"#);
    let shut_resp = read_rpc(&mut reader).expect("Expected shutdown response");
    assert!(shut_resp.contains(r#""id":5"#));
    assert!(shut_resp.contains(r#""result":null"#));

    send_rpc(&mut stdin, r#"{"jsonrpc":"2.0","method":"exit"}"#);
    drop(stdin);
    let status = child.wait().expect("Failed to wait on child");
    assert!(status.success());
}
