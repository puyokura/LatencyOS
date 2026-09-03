//! Static include preprocessor for PulseLang scripts.

#[cfg(any(feature = "alloc", test))]
extern crate alloc;

use crate::error::CompileError;

const MAX_INCLUDE_DEPTH: usize = 8;

/// Preprocess `@include "file.pul";` directives by expanding file contents at text level.
#[cfg(any(feature = "alloc", test))]
pub fn preprocess_includes<F>(
    root_src: &str,
    resolver: &mut F,
) -> Result<alloc::string::String, CompileError>
where
    F: FnMut(&str) -> Option<alloc::string::String>,
{
    let mut call_stack = alloc::vec::Vec::new();
    preprocess_recursive(root_src, "<root>", resolver, &mut call_stack, 0)
}

#[cfg(any(feature = "alloc", test))]
fn preprocess_recursive<F>(
    src: &str,
    current_file: &str,
    resolver: &mut F,
    call_stack: &mut alloc::vec::Vec<alloc::string::String>,
    depth: usize,
) -> Result<alloc::string::String, CompileError>
where
    F: FnMut(&str) -> Option<alloc::string::String>,
{
    if depth > MAX_INCLUDE_DEPTH {
        return Err(CompileError::simple(
            "ERR_MAX_INCLUDE_DEPTH",
            "Maximum @include depth exceeded (max 8 levels limit reached)",
        ));
    }

    call_stack.push(alloc::string::String::from(current_file));

    let mut result = alloc::string::String::new();
    let bytes = src.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i..].starts_with(b"@include") {
            let start_pos = i;
            i += 8; // skip "@include"
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'"' {
                i += 1; // skip opening quote
                let path_start = i;
                while i < bytes.len() && bytes[i] != b'"' && bytes[i] != b'\n' && bytes[i] != b'\r' {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'"' {
                    let inc_path = &src[path_start..i];
                    i += 1; // skip closing quote
                    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                        i += 1;
                    }
                    if i < bytes.len() && bytes[i] == b';' {
                        i += 1; // skip ';'
                    }

                    // Check circular include
                    if call_stack.iter().any(|p| p == inc_path) {
                        return Err(CompileError::simple(
                            "ERR_CIRCULAR_INCLUDE",
                            "Circular @include dependency detected in script",
                        ));
                    }

                    // Resolve file content
                    let inc_content = match resolver(inc_path) {
                        Some(c) => c,
                        None => {
                            return Err(CompileError::simple(
                                "ERR_INCLUDE_NOT_FOUND",
                                "Included file could not be found or loaded by resolver",
                            ));
                        }
                    };

                    // Recursively expand included file
                    let expanded_child = preprocess_recursive(
                        &inc_content,
                        inc_path,
                        resolver,
                        call_stack,
                        depth + 1,
                    )?;
                    result.push_str(&expanded_child);
                    if !expanded_child.ends_with('\n') {
                        result.push('\n');
                    }
                    continue;
                }
            }
            // If not valid @include syntax, push raw text
            i = start_pos;
            result.push(bytes[i] as char);
            i += 1;
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    call_stack.pop();
    Ok(result)
}
