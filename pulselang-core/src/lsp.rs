//! PulseLang Language Server Protocol (LSP) Engine
//!
//! Implements JSON-RPC 2.0 communication over stdio for editors (VSCode, Zed, Neovim).
//! Provides diagnostics, inline WCET estimation, hover information, completions, and code actions.

#[cfg(feature = "std")]
use std::collections::HashMap;
#[cfg(feature = "std")]
use std::format;
#[cfg(feature = "std")]
use std::io::{self, BufRead, Write};
#[cfg(feature = "std")]
use std::string::{String, ToString};
#[cfg(feature = "std")]
use std::vec;
#[cfg(feature = "std")]
use std::vec::Vec;

#[cfg(feature = "std")]
pub struct LspServer {
    documents: HashMap<String, String>,
    shutdown_requested: bool,
}

#[cfg(feature = "std")]
impl LspServer {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            shutdown_requested: false,
        }
    }

    /// Read next JSON-RPC 2.0 message from input stream
    pub fn read_message<R: BufRead>(reader: &mut R) -> io::Result<Option<String>> {
        let mut content_length: Option<usize> = None;
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line)?;
            if bytes_read == 0 {
                return Ok(None); // EOF
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                // Header section ended
                break;
            }
            if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
                if let Ok(len) = rest.trim().parse::<usize>() {
                    content_length = Some(len);
                }
            }
        }

        let len = match content_length {
            Some(l) => l,
            None => return Err(io::Error::new(io::ErrorKind::InvalidData, "Missing Content-Length header")),
        };

        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf)?;
        let msg = String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Some(msg))
    }

    /// Send JSON-RPC 2.0 message to output stream
    pub fn write_message<W: Write>(writer: &mut W, body: &str) -> io::Result<()> {
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        writer.write_all(header.as_bytes())?;
        writer.write_all(body.as_bytes())?;
        writer.flush()?;
        Ok(())
    }

    /// Dispatch incoming JSON-RPC request/notification and return response body if applicable
    pub fn handle_request(&mut self, request_json: &str) -> Option<String> {
        let req = parse_json_obj(request_json)?;
        let method = req.get("method")?.as_str()?;
        let id_val = req.get("id").cloned();
        let id_str = id_val.as_ref().map(|v| v.to_json_string());

        match method {
            "initialize" => {
                let id = id_str?;
                Some(format!(
                    "{{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":{{\"capabilities\":{{\"textDocumentSync\":1,\"hoverProvider\":true,\"completionProvider\":{{\"resolveProvider\":false,\"triggerCharacters\":[\"@\",\"$\",\"#\",\":\"]}},\"codeActionProvider\":true,\"documentFormattingProvider\":true,\"semanticTokensProvider\":{{\"legend\":{{\"tokenTypes\":[\"keyword\",\"variable\",\"parameter\",\"type\",\"function\",\"macro\",\"number\",\"string\",\"operator\",\"comment\"],\"tokenModifiers\":[\"declaration\",\"readonly\"]}},\"full\":true}}}}}}}}",
                    id
                ))
            }
            "initialized" => None,
            "shutdown" => {
                self.shutdown_requested = true;
                let id = id_str?;
                Some(format!(r#"{{"jsonrpc":"2.0","id":{},"result":null}}"#, id))
            }
            "exit" => None,
            "textDocument/didOpen" => {
                let params = req.get("params")?;
                let doc = params.get("textDocument")?;
                let uri = doc.get("uri")?.as_str()?.to_string();
                let text = doc.get("text")?.as_str()?.to_string();
                self.documents.insert(uri.clone(), text);
                self.publish_diagnostics(&uri)
            }
            "textDocument/didChange" => {
                let params = req.get("params")?;
                let doc = params.get("textDocument")?;
                let uri = doc.get("uri")?.as_str()?.to_string();
                let changes = params.get("contentChanges")?.as_array()?;
                if let Some(last_change) = changes.last() {
                    if let Some(text) = last_change.get("text").and_then(|t| t.as_str()) {
                        self.documents.insert(uri.clone(), text.to_string());
                        return self.publish_diagnostics(&uri);
                    }
                }
                None
            }
            "textDocument/didSave" => {
                let params = req.get("params")?;
                let doc = params.get("textDocument")?;
                let uri = doc.get("uri")?.as_str()?;
                self.publish_diagnostics(uri)
            }
            "textDocument/hover" => {
                let id = id_str?;
                let params = req.get("params")?;
                let doc = params.get("textDocument")?;
                let uri = doc.get("uri")?.as_str()?;
                let pos = params.get("position")?;
                let line = pos.get("line")?.as_u64()? as usize;
                let character = pos.get("character")?.as_u64()? as usize;

                let hover_info = self.compute_hover(uri, line, character);
                let result = match hover_info {
                    Some(info) => format!(
                        r#"{{"contents":{{"kind":"markdown","value":"{}"}}}}"#,
                        escape_json(&info)
                    ),
                    None => "null".to_string(),
                };
                Some(format!(r#"{{"jsonrpc":"2.0","id":{},"result":{}}}"#, id, result))
            }
            "textDocument/completion" => {
                let id = id_str?;
                let completions = self.compute_completions();
                Some(format!(r#"{{"jsonrpc":"2.0","id":{},"result":{}}}"#, id, completions))
            }
            "textDocument/codeAction" => {
                let id = id_str?;
                let params = req.get("params")?;
                let doc = params.get("textDocument")?;
                let uri = doc.get("uri")?.as_str()?;
                let actions = self.compute_code_actions(uri);
                Some(format!(r#"{{"jsonrpc":"2.0","id":{},"result":{}}}"#, id, actions))
            }
            "textDocument/formatting" => {
                let id = id_str?;
                let params = req.get("params")?;
                let doc = params.get("textDocument")?;
                let uri = doc.get("uri")?.as_str()?;
                let edits = self.compute_formatting(uri);
                Some(format!(r#"{{"jsonrpc":"2.0","id":{},"result":{}}}"#, id, edits))
            }
            "textDocument/semanticTokens/full" => {
                let id = id_str?;
                let params = req.get("params")?;
                let doc = params.get("textDocument")?;
                let uri = doc.get("uri")?.as_str()?;
                let data = self.compute_semantic_tokens(uri);
                Some(format!(r#"{{"jsonrpc":"2.0","id":{},"result":{{"data":[{}]}}}}"#, id, data))
            }
            _ => {
                if let Some(id) = id_str {
                    Some(format!(
                        r#"{{"jsonrpc":"2.0","id":{},"error":{{"code":-32601,"message":"Method not found: {}"}}}}"#,
                        id, method
                    ))
                } else {
                    None
                }
            }
        }
    }

    /// Compile document and generate `textDocument/publishDiagnostics` notification
    pub fn publish_diagnostics(&self, uri: &str) -> Option<String> {
        let text = self.documents.get(uri)?;
        let mut diags = Vec::new();

        // Run compiler checks
        let mut out_bin = [0u8; 1024];
        if let Err(err) = crate::compile_pulse_to_binary(text.as_bytes(), &mut out_bin) {
            diags.push(self.format_lsp_diagnostic(&err));
        }

        Some(format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{{"uri":"{}","diagnostics":[{}]}}}}"#,
            escape_json(uri),
            diags.join(",")
        ))
    }

    fn format_lsp_diagnostic(&self, err: &crate::error::CompileError) -> String {
        let line = err.line.saturating_sub(1);
        let col = err.col.saturating_sub(1);
        let end_col = col + err.token_len.max(1);

        let msg = format!(
            "[{}] {}\nStage: {}\nAI Hint: {}",
            err.code, err.message, err.stage, err.suggestion
        );

        format!(
            r#"{{"range":{{"start":{{"line":{},"character":{}}},"end":{{"line":{},"character":{}}}}},"severity":1,"code":"{}","source":"pulc","message":"{}"}}"#,
            line, col, line, end_col, err.code, escape_json(&msg)
        )
    }

    fn compute_hover(&self, uri: &str, line: usize, character: usize) -> Option<String> {
        let text = self.documents.get(uri)?;
        let lines: Vec<&str> = text.lines().collect();
        if line >= lines.len() {
            return None;
        }
        let cur_line = lines[line];
        if character >= cur_line.len() {
            return None;
        }

        let word = extract_word_at(cur_line, character);
        match word.as_str() {
            "@tsc" => Some("### `@tsc() -> i64`\n\nReads hardware Timestamp Counter (TSC) cycle count.\n\n- **WCET**: 25 ns\n- **Category**: Hardware Time Intrinsic\n- **Requires**: `@import \"sys\";`".to_string()),
            "@uptime_ns" => Some("### `@uptime_ns() -> i64`\n\nReturns nanoseconds elapsed since OS boot.\n\n- **WCET**: 35 ns\n- **Category**: System Time Intrinsic\n- **Requires**: `@import \"sys\";`".to_string()),
            "@core_id" => Some("### `@core_id() -> i64`\n\nReturns active CPU core index (0..3).\n\n- **WCET**: 15 ns\n- **Category**: Multicore Intrinsic\n- **Requires**: `@import \"sys\";`".to_string()),
            "@contract" => Some("### `@contract: @wcet(<bound>) @budget(<bound>);`\n\nDeclares static worst-case execution time (WCET) constraint and dynamic runtime budget.".to_string()),
            "@requires" => Some("### `@requires(<expr>)`\n\nDesign-by-Contract precondition check. Verified on function entry.".to_string()),
            "@ensures" => Some("### `@ensures(<expr>)`\n\nDesign-by-Contract postcondition check. Evaluates `$result` against assertion before return.".to_string()),
            "@test" => Some("### `@test \"name\" @budget(<time>) { ... }`\n\nNative unit test declaration with real-time budget guard.".to_string()),
            "@println" => Some("### `@println($val: any) -> void`\n\nOutputs string, variable, or literal to serial/screen.\n\n- **WCET**: 5,000 ns".to_string()),
            "@print" => Some("### `@print($val: any) -> void`\n\nOutputs string or variable without newline.\n\n- **WCET**: 5,000 ns".to_string()),
            "@assert" => Some("### `@assert(<condition>)`\n\nTrap CPU if condition is false (`ERR_PX64_ASSERTION_FAILED`).\n\n- **WCET**: 10 ns".to_string()),
            "@send" => Some("### `@send(#handle) -> void`\n\nLinear type consumption. Sends network/hardware packet.\n\n- **Linearity**: Consumes `#handle` exactly once.".to_string()),
            "@capture" => Some("### `@capture() -> #handle`\n\nZero-copy hardware frame capture. Returns linear handle.\n\n- **Linearity**: Must be consumed via `@send` or drop.".to_string()),
            _ => {
                if word.starts_with('$') {
                    Some(format!("### Variable `{}`\n\nPulseLang 64-bit integer / fixed-point register.", word))
                } else if word.starts_with('#') {
                    Some(format!("### Linear Hardware Handle `{}`\n\nMust be consumed exactly once without leaks or double-sends.", word))
                } else {
                    None
                }
            }
        }
    }

    fn compute_completions(&self) -> String {
        r#"[
            {"label":"@contract","kind":15,"detail":"Static WCET and Dynamic Budget Contract","insertText":"@contract: @wcet(${1:100us}) @budget(${2:500us});"},
            {"label":"@test","kind":15,"detail":"Native Test Block with Budget","insertText":"@test \"${1:test name}\" @budget(${2:50us}) {\n    $0\n}"},
            {"label":"@requires","kind":15,"detail":"Precondition Contract","insertText":"@requires(${1:$arg >= 0})"},
            {"label":"@ensures","kind":15,"detail":"Postcondition Contract","insertText":"@ensures(${1:$result >= 0})"},
            {"label":"@import","kind":15,"detail":"Runtime Module Import","insertText":"@import \"${1|core,math,sys,net,vram,gpu,all,tiny|}\";"},
            {"label":"@tsc","kind":3,"detail":"Read Timestamp Counter (25ns WCET)","insertText":"@tsc()"},
            {"label":"@uptime_ns","kind":3,"detail":"Read System Uptime in ns (35ns WCET)","insertText":"@uptime_ns()"},
            {"label":"@core_id","kind":3,"detail":"Read Core ID (15ns WCET)","insertText":"@core_id()"},
            {"label":"@println","kind":3,"detail":"Print Line to Output Console","insertText":"@println(${1:$val});"},
            {"label":"@print","kind":3,"detail":"Print to Output Console","insertText":"@print(${1:$val});"},
            {"label":"@assert","kind":3,"detail":"Assert Condition","insertText":"@assert(${1:condition});"},
            {"label":"@capture","kind":3,"detail":"Capture Linear Hardware Frame","insertText":"@capture()"},
            {"label":"@send","kind":3,"detail":"Consume and Send Linear Handle","insertText":"@send(${1:#handle});"},
            {"label":"fn","kind":14,"detail":"Function Declaration","insertText":"fn ${1:name}(${2:$arg: i64}) -> ${3:i64} {\n    $0\n}"},
            {"label":"let","kind":14,"detail":"Immutable Variable Binding","insertText":"let $${1:x} = ${2:0};"},
            {"label":"let mut","kind":14,"detail":"Mutable Variable Binding","insertText":"let mut $${1:x} = ${2:0};"},
            {"label":"match","kind":14,"detail":"Exhaustive Pattern Matching","insertText":"match ${1:$expr} {\n    ${2:Pattern} => ${3:expr},\n}"}
        ]"#.to_string()
    }

    fn compute_code_actions(&self, uri: &str) -> String {
        let text = match self.documents.get(uri) {
            Some(t) => t,
            None => return "[]".to_string(),
        };

        let mut actions = Vec::new();

        // 1. Missing @contract action
        if !text.contains("@contract:") {
            actions.push(format!(
                r#"{{"title":"Add @contract WCET & Budget annotation","kind":"quickfix","edit":{{"changes":{{"{}":[{{"range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":0}}}},"newText":"@contract: @wcet(100us) @budget(500us);\n"}}]}}}}}}"#,
                escape_json(uri)
            ));
        }

        // 2. Missing @import "sys" if @tsc or @uptime_ns is used without import
        if (text.contains("@tsc") || text.contains("@uptime_ns")) && !text.contains("@import") {
            actions.push(format!(
                r#"{{"title":"Add @import \"sys\"; for time intrinsics","kind":"quickfix","edit":{{"changes":{{"{}":[{{"range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":0}}}},"newText":"@import \"sys\";\n"}}]}}}}}}"#,
                escape_json(uri)
            ));
        }

        format!("[{}]", actions.join(","))
    }

    fn compute_formatting(&self, uri: &str) -> String {
        let text = match self.documents.get(uri) {
            Some(t) => t,
            None => return "[]".to_string(),
        };

        let formatted = match crate::fmt::format_source(text) {
            Ok(f) => f,
            Err(_) => return "[]".to_string(),
        };

        if formatted == *text {
            return "[]".to_string();
        }

        let lines = text.lines().count();
        format!(
            r#"[{{"range":{{"start":{{"line":0,"character":0}},"end":{{"line":{},"character":0}}}},"newText":"{}"}}]"#,
            lines + 1,
            escape_json(&formatted)
        )
    }
#[cfg(feature = "std")]
    fn compute_semantic_tokens(&self, uri: &str) -> String {
        let text = match self.documents.get(uri) {
            Some(t) => t,
            None => return String::new(),
        };

        // Token types legend:
        // 0: keyword
        // 1: variable
        // 2: parameter
        // 3: type
        // 4: function
        // 5: macro
        // 6: number
        // 7: string
        // 8: operator
        // 9: comment

        let mut tokens = vec![crate::token::Token::empty(); 4096];
        let mut lexer = crate::lexer::Lexer::new(text.as_bytes());
        let count = match lexer.tokenize(&mut tokens) {
            Ok(c) => c,
            Err(_) => return String::new(),
        };

        let mut data: Vec<u32> = Vec::new();
        let mut prev_line: u32 = 0;
        let mut prev_col: u32 = 0;

        for tok in &tokens[..count] {
            if tok.kind == crate::token::TokenKind::Eof || tok.len == 0 {
                continue;
            }

            let tok_type: Option<u32> = match tok.kind {
                crate::token::TokenKind::Let
                | crate::token::TokenKind::Mut
                | crate::token::TokenKind::Match
                | crate::token::TokenKind::If
                | crate::token::TokenKind::Else
                | crate::token::TokenKind::While
                | crate::token::TokenKind::Within
                | crate::token::TokenKind::For
                | crate::token::TokenKind::In
                | crate::token::TokenKind::Fn
                | crate::token::TokenKind::Struct
                | crate::token::TokenKind::Const
                | crate::token::TokenKind::Enum
                | crate::token::TokenKind::Return
                | crate::token::TokenKind::Drop => Some(0), // keyword

                crate::token::TokenKind::VarIdent => Some(1), // variable
                crate::token::TokenKind::HardwareIdent => Some(2), // parameter/hardware slot

                crate::token::TokenKind::Fixed
                | crate::token::TokenKind::I64
                | crate::token::TokenKind::U8
                | crate::token::TokenKind::U16
                | crate::token::TokenKind::U32
                | crate::token::TokenKind::U64 => Some(3), // type

                crate::token::TokenKind::Ident => Some(4), // function / identifier

                crate::token::TokenKind::AtContract
                | crate::token::TokenKind::AtPipeline
                | crate::token::TokenKind::AtBudget
                | crate::token::TokenKind::AtWcet
                | crate::token::TokenKind::AtWithin
                | crate::token::TokenKind::AtWhile
                | crate::token::TokenKind::AtFor
                | crate::token::TokenKind::AtLoop
                | crate::token::TokenKind::AtOnVblank
                | crate::token::TokenKind::AtAssert
                | crate::token::TokenKind::AtRequires
                | crate::token::TokenKind::AtEnsures
                | crate::token::TokenKind::AtInvariant
                | crate::token::TokenKind::AtPoolSize
                | crate::token::TokenKind::AtTest
                | crate::token::TokenKind::AtImport
                | crate::token::TokenKind::IntrinsicIdent => Some(5), // macro / intrinsic

                crate::token::TokenKind::Number(_)
                | crate::token::TokenKind::FloatLit(_, _)
                | crate::token::TokenKind::TimeLiteral(_) => Some(6), // number

                crate::token::TokenKind::StringLit => Some(7), // string

                crate::token::TokenKind::ColonEq
                | crate::token::TokenKind::PlusEq
                | crate::token::TokenKind::MinusEq
                | crate::token::TokenKind::Pipe
                | crate::token::TokenKind::EqEq
                | crate::token::TokenKind::NotEq
                | crate::token::TokenKind::LtEq
                | crate::token::TokenKind::GtEq
                | crate::token::TokenKind::And
                | crate::token::TokenKind::Or => Some(8), // operator

                _ => None,
            };

            if let Some(t_type) = tok_type {
                let cur_line = tok.line.saturating_sub(1) as u32;
                let cur_col = tok.col.saturating_sub(1) as u32;
                let delta_line = cur_line.saturating_sub(prev_line);
                let delta_col = if delta_line == 0 {
                    cur_col.saturating_sub(prev_col)
                } else {
                    cur_col
                };

                data.push(delta_line);
                data.push(delta_col);
                data.push(tok.len as u32);
                data.push(t_type);
                data.push(0); // tokenModifiers

                prev_line = cur_line;
                prev_col = cur_col;
            }
        }

        let strs: Vec<String> = data.iter().map(|n| format!("{}", n)).collect();
        strs.join(",")
    }
}

// -----------------------------------------------------------------------------
// Helper parsing & formatting routines
// -----------------------------------------------------------------------------

fn extract_word_at(line: &str, character: usize) -> String {
    let bytes = line.as_bytes();
    if character >= bytes.len() {
        return String::new();
    }
    let is_word_char = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'@' || b == b'$' || b == b'#';

    let mut start = character;
    while start > 0 && is_word_char(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = character;
    while end < bytes.len() && is_word_char(bytes[end]) {
        end += 1;
    }
    line[start..end].to_string()
}

#[cfg(feature = "std")]
fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

// Minimalistic JSON Value AST for zero-dependency parsing
#[cfg(feature = "std")]
#[derive(Debug, Clone)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(HashMap<String, JsonValue>),
}

#[cfg(feature = "std")]
impl JsonValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            JsonValue::Number(n) => Some(*n as u64),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&Vec<JsonValue>> {
        match self {
            JsonValue::Array(a) => Some(a),
            _ => None,
        }
    }
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(map) => map.get(key),
            _ => None,
        }
    }
    pub fn to_json_string(&self) -> String {
        match self {
            JsonValue::Null => "null".to_string(),
            JsonValue::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
            JsonValue::Number(n) => format!("{}", n),
            JsonValue::String(s) => format!("\"{}\"", escape_json(s)),
            JsonValue::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|item| item.to_json_string()).collect();
                format!("[{}]", items.join(","))
            }
            JsonValue::Object(map) => {
                let items: Vec<String> = map.iter().map(|(k, v)| format!("\"{}\":{}", escape_json(k), v.to_json_string())).collect();
                format!("{{{}}}", items.join(","))
            }
        }
    }
}

#[cfg(feature = "std")]
pub fn parse_json_obj(s: &str) -> Option<HashMap<String, JsonValue>> {
    let v = parse_json_value(s.trim())?;
    match v {
        JsonValue::Object(m) => Some(m),
        _ => None,
    }
}

#[cfg(feature = "std")]
fn parse_json_value(s: &str) -> Option<JsonValue> {
    let s = s.trim();
    if s == "null" {
        return Some(JsonValue::Null);
    }
    if s == "true" {
        return Some(JsonValue::Bool(true));
    }
    if s == "false" {
        return Some(JsonValue::Bool(false));
    }
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        // Unescape string
        let content = &s[1..s.len() - 1];
        let mut unescaped = String::new();
        let mut chars = content.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('"') => unescaped.push('"'),
                    Some('\\') => unescaped.push('\\'),
                    Some('/') => unescaped.push('/'),
                    Some('b') => unescaped.push('\x08'),
                    Some('f') => unescaped.push('\x0c'),
                    Some('n') => unescaped.push('\n'),
                    Some('r') => unescaped.push('\r'),
                    Some('t') => unescaped.push('\t'),
                    Some(other) => unescaped.push(other),
                    None => unescaped.push('\\'),
                }
            } else {
                unescaped.push(c);
            }
        }
        return Some(JsonValue::String(unescaped));
    }
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1].trim();
        if inner.is_empty() {
            return Some(JsonValue::Array(Vec::new()));
        }
        let items = split_json_items(inner);
        let mut arr = Vec::new();
        for item in &items {
            arr.push(parse_json_value(item)?);
        }
        return Some(JsonValue::Array(arr));
    }
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1].trim();
        let mut map = HashMap::new();
        if inner.is_empty() {
            return Some(JsonValue::Object(map));
        }
        let pairs = split_json_items(inner);
        for pair in &pairs {
            let parts: Vec<&str> = pair.splitn(2, ':').collect();
            if parts.len() != 2 {
                continue;
            }
            let key = match parse_json_value(parts[0].trim())? {
                JsonValue::String(k) => k,
                _ => continue,
            };
            let val = parse_json_value(parts[1].trim())?;
            map.insert(key, val);
        }
        return Some(JsonValue::Object(map));
    }
    if let Ok(num) = s.parse::<f64>() {
        return Some(JsonValue::Number(num));
    }
    None
}

#[cfg(feature = "std")]
fn split_json_items(s: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut depth = 0;
    let mut in_str = false;
    let mut escape = false;
    let mut cur = String::new();

    for c in s.chars() {
        if escape {
            cur.push(c);
            escape = false;
            continue;
        }
        if c == '\\' && in_str {
            cur.push(c);
            escape = true;
            continue;
        }
        if c == '"' {
            in_str = !in_str;
            cur.push(c);
            continue;
        }
        if !in_str {
            if c == '{' || c == '[' {
                depth += 1;
            } else if c == '}' || c == ']' {
                depth -= 1;
            } else if c == ',' && depth == 0 {
                items.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        items.push(cur.trim().to_string());
    }
    items
}
