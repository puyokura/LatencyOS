//! PulseLang single-pass compiler for px64 register virtual architecture

use crate::error::CompileError;
use crate::isa::*;
use crate::token::{Token, TokenKind};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HandleState {
    Unallocated,
    Allocated { line: usize, col: usize },
    Consumed,
}

#[derive(Clone, Copy)]
pub struct FnMeta {
    pub name: [u8; 16],
    pub name_len: usize,
    pub entry_pc: u16,
    pub param_names: [[u8; 16]; 4],
    pub param_lens: [usize; 4],
    pub param_count: u8,
}

impl FnMeta {
    pub const fn empty() -> Self {
        Self {
            name: [0; 16],
            name_len: 0,
            entry_pc: 0,
            param_names: [[0; 16]; 4],
            param_lens: [0; 4],
            param_count: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ArrayMeta {
    pub name: [u8; 16],
    pub name_len: usize,
    pub arr_id: u8,
    pub base: u16,
    pub len: u16,
}

impl ArrayMeta {
    pub const fn empty() -> Self {
        Self {
            name: [0; 16],
            name_len: 0,
            arr_id: 0,
            base: 0,
            len: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct StructFieldMeta {
    pub name: [u8; 16],
    pub name_len: usize,
    pub offset: u8,
}

impl StructFieldMeta {
    pub const fn empty() -> Self {
        Self {
            name: [0; 16],
            name_len: 0,
            offset: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct StructDefMeta {
    pub name: [u8; 16],
    pub name_len: usize,
    pub fields: [StructFieldMeta; 8],
    pub field_count: u8,
}

impl StructDefMeta {
    pub const fn empty() -> Self {
        Self {
            name: [0; 16],
            name_len: 0,
            fields: [StructFieldMeta::empty(); 8],
            field_count: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct StructInstMeta {
    pub var_name: [u8; 16],
    pub var_name_len: usize,
    pub struct_def_idx: u8,
    pub inst_id: u8,
    pub base_slot: u16,
    pub field_count: u8,
}

impl StructInstMeta {
    pub const fn empty() -> Self {
        Self {
            var_name: [0; 16],
            var_name_len: 0,
            struct_def_idx: 0,
            inst_id: 0,
            base_slot: 0,
            field_count: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ConstTableMeta {
    pub name: [u8; 16],
    pub name_len: usize,
    pub tbl_id: u8,
    pub base_idx: u8,
    pub len: u8,
}

impl ConstTableMeta {
    pub const fn empty() -> Self {
        Self {
            name: [0; 16],
            name_len: 0,
            tbl_id: 0,
            base_idx: 0,
            len: 0,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ViewMeta {
    pub name: [u8; 16],
    pub name_len: usize,
    pub arr_id: u8,
    pub base_reg: u8,
    pub stride_imm: u16,
    pub len_imm: u16,
}

impl ViewMeta {
    pub const fn empty() -> Self {
        Self {
            name: [0; 16],
            name_len: 0,
            arr_id: 0,
            base_reg: 0,
            stride_imm: 1,
            len_imm: 0,
        }
    }
}

/// Compilation summary statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompileStats {
    pub code_size: usize,
    pub instruction_count: usize,
    pub str_pool_len: usize,
    pub const_pool_len: usize,
    pub var_count: usize,
    pub function_count: usize,
    pub array_count: usize,
    pub struct_def_count: usize,
    pub struct_inst_count: usize,
    pub const_table_count: usize,
    pub total_binary_size: usize,
}
/// Single-pass compiler for px64 architecture.
pub struct Compiler<'a> {
    pub src: &'a [u8],
    pub tokens: &'a [Token],
    pub current: usize,
    pub code: [u8; MAX_BYTECODE_SIZE],
    pub code_len: usize,
    pub var_names: [[u8; 16]; MAX_VARS],
    pub var_lens: [usize; MAX_VARS],
    pub var_regs: [u8; MAX_VARS],
    pub var_mut: [bool; MAX_VARS],
    pub var_count: usize,
    pub str_pool: [u8; MAX_STRING_POOL],
    pub str_pool_len: usize,
    pub const_pool: [i64; MAX_CONST_POOL],
    pub const_pool_len: usize,
    temp_used: u8,
    handle_states: [HandleState; 4],
    pub arrays: [ArrayMeta; 8],
    pub array_count: usize,
    pub total_array_elements: usize,
    pub functions: [FnMeta; 16],
    pub fn_count: usize,
    pub current_fn: Option<usize>,
    pub struct_defs: [StructDefMeta; 8],
    pub struct_def_count: usize,
    pub struct_insts: [StructInstMeta; 8],
    pub struct_inst_count: usize,
    pub total_struct_fields: usize,
    pub const_tables: [ConstTableMeta; 8],
    pub const_table_count: usize,
    pub views: [ViewMeta; 8],
    pub view_count: usize,
}

impl<'a> Compiler<'a> {
    pub fn new(src: &'a [u8], tokens: &'a [Token]) -> Self {
        Self {
            src,
            tokens,
            current: 0,
            code: [0; MAX_BYTECODE_SIZE],
            code_len: 0,
            var_names: [[0; 16]; MAX_VARS],
            var_lens: [0; MAX_VARS],
            var_regs: [0; MAX_VARS],
            var_mut: [true; MAX_VARS],
            var_count: 0,
            str_pool: [0; MAX_STRING_POOL],
            str_pool_len: 0,
            const_pool: [0; MAX_CONST_POOL],
            const_pool_len: 0,
            temp_used: 0,
            handle_states: [HandleState::Unallocated; 4],
            arrays: [ArrayMeta::empty(); 8],
            array_count: 0,
            total_array_elements: 0,
            functions: [FnMeta::empty(); 16],
            fn_count: 0,
            current_fn: None,
            struct_defs: [StructDefMeta::empty(); 8],
            struct_def_count: 0,
            struct_insts: [StructInstMeta::empty(); 8],
            struct_inst_count: 0,
            total_struct_fields: 0,
            const_tables: [ConstTableMeta::empty(); 8],
            const_table_count: 0,
            views: [ViewMeta::empty(); 8],
            view_count: 0,
        }
    }

    /// Retrieve compilation statistics.
    pub fn stats(&self) -> CompileStats {
        let const_pool_bytes = self.const_pool_len * 8;
        let total_binary_size = PX64_HEADER_SIZE + self.code_len + self.str_pool_len + const_pool_bytes;
        CompileStats {
            code_size: self.code_len,
            instruction_count: self.code_len / 4,
            str_pool_len: self.str_pool_len,
            const_pool_len: self.const_pool_len,
            var_count: self.var_count,
            function_count: self.fn_count,
            array_count: self.array_count,
            struct_def_count: self.struct_def_count,
            struct_inst_count: self.struct_inst_count,
            const_table_count: self.const_table_count,
            total_binary_size,
        }
    }

    pub fn declare_array(&mut self, tok: Token, len: usize) -> Result<u8, CompileError> {
        let name = &self.src[tok.start..tok.start + tok.len];
        if self.array_count >= 8 {
            return Err(self.error(
                "ERR_MAX_ARRAYS_EXCEEDED",
                "Maximum distinct arrays limit reached (8 arrays limit)",
                "Fewer distinct arrays",
                "Array Allocation",
                "Reduce distinct array declarations across script",
            ));
        }
        if self.total_array_elements + len > 256 {
            return Err(self.error(
                "ERR_ARRAY_CAPACITY_EXCEEDED",
                "Total static array capacity exceeded (max 256 elements)",
                "Smaller array size",
                "Array Allocation",
                "Reduce total array elements in script",
            ));
        }
        let arr_id = self.array_count as u8;
        let mut meta = ArrayMeta::empty();
        meta.name_len = core::cmp::min(name.len(), 16);
        meta.name[..meta.name_len].copy_from_slice(&name[..meta.name_len]);
        meta.arr_id = arr_id;
        meta.base = self.total_array_elements as u16;
        meta.len = len as u16;
        self.arrays[self.array_count] = meta;
        self.array_count += 1;
        self.total_array_elements += len;
        Ok(arr_id)
    }

    pub fn lookup_array(&self, tok: Token) -> Result<u8, CompileError> {
        let name = &self.src[tok.start..tok.start + tok.len];
        for i in 0..self.array_count {
            let meta = &self.arrays[i];
            if meta.name_len == name.len() && &meta.name[..meta.name_len] == name {
                return Ok(meta.arr_id);
            }
        }
        Err(self.error(
            "ERR_ARRAY_UNDEFINED",
            "Array variable is not defined",
            "Declare array with 'let $buf: [i64; N];'",
            "Array Access",
            "Define array buffer before indexing elements",
        ))
    }

    pub fn declare_view(&mut self, tok: Token, arr_id: u8, base_reg: u8, stride_imm: u16, len_imm: u16) -> Result<(), CompileError> {
        let name = &self.src[tok.start..tok.start + tok.len];
        if self.view_count >= 8 {
            return Err(self.error(
                "ERR_MAX_VIEWS_EXCEEDED",
                "Maximum distinct array views limit reached (8 views limit)",
                "Fewer distinct array views",
                "View Declaration",
                "Reuse view variables across script",
            ));
        }
        for i in 0..self.view_count {
            let v = &self.views[i];
            if v.name_len == name.len() && &v.name[..v.name_len] == name {
                self.views[i] = ViewMeta {
                    name: v.name,
                    name_len: v.name_len,
                    arr_id,
                    base_reg,
                    stride_imm,
                    len_imm,
                };
                return Ok(());
            }
        }
        let mut meta = ViewMeta::empty();
        meta.name_len = core::cmp::min(name.len(), 16);
        meta.name[..meta.name_len].copy_from_slice(&name[..meta.name_len]);
        meta.arr_id = arr_id;
        meta.base_reg = base_reg;
        meta.stride_imm = stride_imm;
        meta.len_imm = len_imm;
        self.views[self.view_count] = meta;
        self.view_count += 1;
        Ok(())
    }

    pub fn lookup_view(&self, tok: Token) -> Option<ViewMeta> {
        let name = &self.src[tok.start..tok.start + tok.len];
        for i in 0..self.view_count {
            let v = &self.views[i];
            if v.name_len == name.len() && &v.name[..v.name_len] == name {
                return Some(*v);
            }
        }
        None
    }

    pub fn is_view(&self, tok: Token) -> bool {
        self.lookup_view(tok).is_some()
    }

    pub fn lookup_fn_by_name(&self, name: &[u8]) -> Result<FnMeta, CompileError> {
        for i in 0..self.fn_count {
            let meta = &self.functions[i];
            if meta.name_len == name.len() && &meta.name[..meta.name_len] == name {
                return Ok(*meta);
            }
        }
        Err(self.error(
            "ERR_UNKNOWN_FUNCTION",
            "Function name passed to combinator is not defined",
            "Valid function name",
            "Combinator -> Function Lookup",
            "Define static function with 'fn name(...) { ... }' before using in combinators",
        ))
    }
    pub fn parse_view_source_to_reg(&mut self, base_reg: u8) -> Result<ViewMeta, CompileError> {
        if self.peek().kind == TokenKind::IntrinsicIdent || self.peek().kind == TokenKind::Ident {
            let tok = self.peek();
            let name = &self.src[tok.start..tok.start + tok.len];
            if name == b"@row" || name == b"@col" || name == b"@slice" {
                self.advance(); // consume intrinsic
                self.match_token(TokenKind::LParen);
                let arr_tok = self.advance();
                let arr_id = self.lookup_array(arr_tok)?;
                let total_len = self.arrays[arr_id as usize].len;

                self.match_token(TokenKind::Comma);
                self.expression(base_reg)?;

                if name == b"@row" {
                    self.match_token(TokenKind::Comma);
                    let cols_tok = self.advance();
                    let cols = match cols_tok.kind {
                        TokenKind::Number(n) if n > 0 => n as usize,
                        _ => 3,
                    };
                    self.match_token(TokenKind::RParen);

                    let cols_reg = self.alloc_temp()?;
                    self.emit_inst(PX64_OP_MOV_IMM, cols_reg, (cols >> 8) as u8, (cols & 0xff) as u8)?;
                    self.emit_inst(PX64_OP_MUL, base_reg, base_reg, cols_reg)?;
                    self.free_temp(cols_reg);

                    return Ok(ViewMeta {
                        name: [0; 16],
                        name_len: 0,
                        arr_id,
                        base_reg,
                        stride_imm: 1,
                        len_imm: cols as u16,
                    });
                } else if name == b"@col" {
                    self.match_token(TokenKind::Comma);
                    let cols_tok = self.advance();
                    let cols = match cols_tok.kind {
                        TokenKind::Number(n) if n > 0 => n as usize,
                        _ => 3,
                    };
                    self.match_token(TokenKind::RParen);

                    let rows = (total_len as usize) / cols;

                    return Ok(ViewMeta {
                        name: [0; 16],
                        name_len: 0,
                        arr_id,
                        base_reg,
                        stride_imm: cols as u16,
                        len_imm: rows as u16,
                    });
                } else {
                    // @slice($arr, $start, $len)
                    self.match_token(TokenKind::Comma);
                    let len_tok = self.advance();
                    let len = match len_tok.kind {
                        TokenKind::Number(n) if n > 0 => n as usize,
                        _ => 1,
                    };
                    self.match_token(TokenKind::RParen);

                    return Ok(ViewMeta {
                        name: [0; 16],
                        name_len: 0,
                        arr_id,
                        base_reg,
                        stride_imm: 1,
                        len_imm: len as u16,
                    });
                }
            }
        }

        let tok = self.advance();
        if let Some(view) = self.lookup_view(tok) {
            return Ok(view);
        }

        let arr_id = self.lookup_array(tok)?;
        let len = self.arrays[arr_id as usize].len;
        self.emit_inst(PX64_OP_MOV_IMM, base_reg, 0, 0)?;

        Ok(ViewMeta {
            name: [0; 16],
            name_len: 0,
            arr_id,
            base_reg,
            stride_imm: 1,
            len_imm: len,
        })
    }

    pub fn parse_view_source(&mut self) -> Result<ViewMeta, CompileError> {
        if self.peek().kind == TokenKind::VarIdent || self.peek().kind == TokenKind::Ident {
            let tok = self.peek();
            let name = &self.src[tok.start..tok.start + tok.len];
            if name != b"@row" && name != b"@col" && name != b"@slice" {
                self.advance();
                if let Some(view) = self.lookup_view(tok) {
                    return Ok(view);
                }
                let arr_id = self.lookup_array(tok)?;
                let len = self.arrays[arr_id as usize].len;
                return Ok(ViewMeta {
                    name: [0; 16],
                    name_len: 0,
                    arr_id,
                    base_reg: 0,
                    stride_imm: 1,
                    len_imm: len,
                });
            }
        }
        let temp_base = self.alloc_temp()?;
        self.parse_view_source_to_reg(temp_base)
    }
    pub fn compile_zip_with_to_reg(
        &mut self,
        dst: u8,
        v1: ViewMeta,
        v2: ViewMeta,
        fn_name: &[u8],
        is_sum: bool,
    ) -> Result<(), CompileError> {
        let fn_meta = self.lookup_fn_by_name(fn_name)?;
        let count = core::cmp::min(v1.len_imm, v2.len_imm);

        if is_sum {
            self.emit_inst(PX64_OP_MOV_IMM, dst, 0, 0)?;
        }

        let loop_i = self.alloc_temp()?;
        self.emit_inst(PX64_OP_MOV_IMM, loop_i, 0, 0)?;

        let count_reg = self.alloc_temp()?;
        self.emit_inst(PX64_OP_MOV_IMM, count_reg, (count >> 8) as u8, (count & 0xff) as u8)?;

        let loop_start_pc = self.code_len;

        let cond_reg = self.alloc_temp()?;
        self.emit_inst(PX64_OP_CMP_LT, cond_reg, loop_i, count_reg)?;
        let jz_pc = self.code_len;
        self.emit_imm16(PX64_OP_JZ, cond_reg, 0)?;
        self.free_temp(cond_reg);

        let idx1_reg = self.alloc_temp()?;
        if v1.stride_imm == 1 {
            self.emit_inst(PX64_OP_ADD, idx1_reg, v1.base_reg, loop_i)?;
        } else {
            let stride1_reg = self.alloc_temp()?;
            self.emit_inst(PX64_OP_MOV_IMM, stride1_reg, (v1.stride_imm >> 8) as u8, (v1.stride_imm & 0xff) as u8)?;
            self.emit_inst(PX64_OP_MUL, idx1_reg, loop_i, stride1_reg)?;
            self.emit_inst(PX64_OP_ADD, idx1_reg, idx1_reg, v1.base_reg)?;
            self.free_temp(stride1_reg);
        }
        let elem1_reg = self.alloc_temp()?;
        self.emit_inst(PX64_OP_ARR_LOAD, elem1_reg, v1.arr_id, idx1_reg)?;
        self.free_temp(idx1_reg);
        self.emit_inst(PX64_OP_MOV_REG, 7, elem1_reg, 0)?;
        self.free_temp(elem1_reg);

        let idx2_reg = self.alloc_temp()?;
        if v2.stride_imm == 1 {
            self.emit_inst(PX64_OP_ADD, idx2_reg, v2.base_reg, loop_i)?;
        } else {
            let stride2_reg = self.alloc_temp()?;
            self.emit_inst(PX64_OP_MOV_IMM, stride2_reg, (v2.stride_imm >> 8) as u8, (v2.stride_imm & 0xff) as u8)?;
            self.emit_inst(PX64_OP_MUL, idx2_reg, loop_i, stride2_reg)?;
            self.emit_inst(PX64_OP_ADD, idx2_reg, idx2_reg, v2.base_reg)?;
            self.free_temp(stride2_reg);
        }
        let elem2_reg = self.alloc_temp()?;
        self.emit_inst(PX64_OP_ARR_LOAD, elem2_reg, v2.arr_id, idx2_reg)?;
        self.free_temp(idx2_reg);
        self.emit_inst(PX64_OP_MOV_REG, 6, elem2_reg, 0)?;
        self.free_temp(elem2_reg);

        self.emit_imm16(PX64_OP_CALL, 0, fn_meta.entry_pc)?;

        if is_sum {
            self.emit_inst(PX64_OP_ADD, dst, dst, 0)?;
        } else {
            self.emit_inst(PX64_OP_MOV_REG, dst, 0, 0)?;
        }

        self.emit_inst(PX64_OP_ADDI, loop_i, loop_i, 1)?;
        self.emit_imm16(PX64_OP_JMP, 0, loop_start_pc as u16)?;

        let loop_end_pc = self.code_len;
        self.code[jz_pc + 2] = (loop_end_pc >> 8) as u8;
        self.code[jz_pc + 3] = (loop_end_pc & 0xff) as u8;

        self.free_temp(count_reg);
        self.free_temp(loop_i);

        Ok(())
    }
    pub fn compile_sum_to_reg(&mut self, dst: u8, v: ViewMeta) -> Result<(), CompileError> {
        let count = v.len_imm;
        self.emit_inst(PX64_OP_MOV_IMM, dst, 0, 0)?;

        let loop_i = self.alloc_temp()?;
        self.emit_inst(PX64_OP_MOV_IMM, loop_i, 0, 0)?;

        let count_reg = self.alloc_temp()?;
        self.emit_inst(PX64_OP_MOV_IMM, count_reg, (count >> 8) as u8, (count & 0xff) as u8)?;

        let loop_start_pc = self.code_len;

        let cond_reg = self.alloc_temp()?;
        self.emit_inst(PX64_OP_CMP_LT, cond_reg, loop_i, count_reg)?;
        let jz_pc = self.code_len;
        self.emit_imm16(PX64_OP_JZ, cond_reg, 0)?;

        let idx_reg = self.alloc_temp()?;
        if v.stride_imm == 1 {
            self.emit_inst(PX64_OP_ADD, idx_reg, v.base_reg, loop_i)?;
        } else {
            let stride_reg = self.alloc_temp()?;
            self.emit_inst(PX64_OP_MOV_IMM, stride_reg, (v.stride_imm >> 8) as u8, (v.stride_imm & 0xff) as u8)?;
            self.emit_inst(PX64_OP_MUL, idx_reg, loop_i, stride_reg)?;
            self.emit_inst(PX64_OP_ADD, idx_reg, idx_reg, v.base_reg)?;
            self.free_temp(stride_reg);
        }
        let elem_reg = self.alloc_temp()?;
        self.emit_inst(PX64_OP_ARR_LOAD, elem_reg, v.arr_id, idx_reg)?;
        self.free_temp(idx_reg);

        self.emit_inst(PX64_OP_ADD, dst, dst, elem_reg)?;
        self.free_temp(elem_reg);

        self.emit_inst(PX64_OP_ADDI, loop_i, loop_i, 1)?;
        self.emit_imm16(PX64_OP_JMP, 0, loop_start_pc as u16)?;

        let loop_end_pc = self.code_len;
        self.code[jz_pc + 2] = (loop_end_pc >> 8) as u8;
        self.code[jz_pc + 3] = (loop_end_pc & 0xff) as u8;

        self.free_temp(cond_reg);
        self.free_temp(count_reg);
        self.free_temp(loop_i);

        Ok(())
    }

    pub fn compile_reduce_to_reg(
        &mut self,
        dst: u8,
        v: ViewMeta,
        init_val: Option<i64>,
        fn_name: &[u8],
    ) -> Result<(), CompileError> {
        let fn_meta = self.lookup_fn_by_name(fn_name)?;
        let count = v.len_imm;

        if let Some(init) = init_val {
            self.emit_const(dst, init)?;
        } else {
            self.emit_inst(PX64_OP_MOV_IMM, dst, 0, 0)?;
        }

        let loop_i = self.alloc_temp()?;
        self.emit_inst(PX64_OP_MOV_IMM, loop_i, 0, 0)?;

        let count_reg = self.alloc_temp()?;
        self.emit_inst(PX64_OP_MOV_IMM, count_reg, (count >> 8) as u8, (count & 0xff) as u8)?;

        let loop_start_pc = self.code_len;

        let cond_reg = self.alloc_temp()?;
        self.emit_inst(PX64_OP_CMP_LT, cond_reg, loop_i, count_reg)?;
        let jz_pc = self.code_len;
        self.emit_imm16(PX64_OP_JZ, cond_reg, 0)?;

        let idx_reg = self.alloc_temp()?;
        if v.stride_imm == 1 {
            self.emit_inst(PX64_OP_ADD, idx_reg, v.base_reg, loop_i)?;
        } else {
            let stride_reg = self.alloc_temp()?;
            self.emit_inst(PX64_OP_MOV_IMM, stride_reg, (v.stride_imm >> 8) as u8, (v.stride_imm & 0xff) as u8)?;
            self.emit_inst(PX64_OP_MUL, idx_reg, loop_i, stride_reg)?;
            self.emit_inst(PX64_OP_ADD, idx_reg, idx_reg, v.base_reg)?;
            self.free_temp(stride_reg);
        }
        let elem_reg = self.alloc_temp()?;
        self.emit_inst(PX64_OP_ARR_LOAD, elem_reg, v.arr_id, idx_reg)?;
        self.free_temp(idx_reg);

        self.emit_inst(PX64_OP_MOV_REG, 7, dst, 0)?;
        self.emit_inst(PX64_OP_MOV_REG, 6, elem_reg, 0)?;
        self.free_temp(elem_reg);

        self.emit_imm16(PX64_OP_CALL, 0, fn_meta.entry_pc)?;
        self.emit_inst(PX64_OP_MOV_REG, dst, 0, 0)?;

        self.emit_inst(PX64_OP_ADDI, loop_i, loop_i, 1)?;
        self.emit_imm16(PX64_OP_JMP, 0, loop_start_pc as u16)?;

        let loop_end_pc = self.code_len;
        self.code[jz_pc + 2] = (loop_end_pc >> 8) as u8;
        self.code[jz_pc + 3] = (loop_end_pc & 0xff) as u8;

        self.free_temp(cond_reg);
        self.free_temp(count_reg);
        self.free_temp(loop_i);

        Ok(())
    }

    pub fn declare_struct_def(&mut self) -> Result<(), CompileError> {
        let name_tok = self.advance();
        if name_tok.kind != TokenKind::Ident {
            return Err(self.error(
                "ERR_EXPECTED_STRUCT_NAME",
                "Expected struct identifier name after 'struct' keyword",
                "Struct name identifier",
                "Statement -> Struct Definition",
                "Specify struct name, e.g. 'struct Point'",
            ));
        }
        let s_name = &self.src[name_tok.start..name_tok.start + name_tok.len];
        if self.struct_def_count >= 8 {
            return Err(self.error(
                "ERR_MAX_STRUCTS_EXCEEDED",
                "Maximum distinct struct definitions reached (8 struct types limit)",
                "Fewer struct definitions",
                "Struct Definition",
                "Reduce distinct struct declarations across script",
            ));
        }

        for i in 0..self.struct_def_count {
            let def = &self.struct_defs[i];
            if def.name_len == s_name.len() && &def.name[..def.name_len] == s_name {
                return Err(self.error(
                    "ERR_STRUCT_REDEFINED",
                    "Struct type is already defined",
                    "Unique struct type name",
                    "Struct Definition",
                    "Rename duplicate struct type",
                ));
            }
        }

        if !self.match_token(TokenKind::LBrace) {
            return Err(self.error(
                "ERR_EXPECTED_LBRACE",
                "Expected opening brace '{' after struct name",
                "Left brace '{'",
                "Statement -> Struct Definition",
                "Add '{' to start struct field list",
            ));
        }

        let mut def = StructDefMeta::empty();
        def.name_len = core::cmp::min(s_name.len(), 16);
        def.name[..def.name_len].copy_from_slice(&s_name[..def.name_len]);

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let f_tok = self.advance();
            if f_tok.kind != TokenKind::Ident {
                return Err(self.error(
                    "ERR_EXPECTED_FIELD_NAME",
                    "Expected field identifier in struct definition",
                    "Field name identifier",
                    "Statement -> Struct Fields",
                    "Specify field name, e.g. 'x: i64'",
                ));
            }
            let f_name = &self.src[f_tok.start..f_tok.start + f_tok.len];
            let f_idx = def.field_count as usize;
            if f_idx >= 8 {
                return Err(self.error(
                    "ERR_MAX_FIELDS_EXCEEDED",
                    "Maximum struct fields reached (8 fields limit per struct)",
                    "Fewer struct fields",
                    "Statement -> Struct Fields",
                    "Reduce field count to 8 or fewer",
                ));
            }

            if !self.match_token(TokenKind::Colon) {
                return Err(self.error(
                    "ERR_EXPECTED_COLON",
                    "Expected ':' after struct field name",
                    "Colon ':'",
                    "Statement -> Struct Fields",
                    "Specify field type annotation 'field: i64'",
                ));
            }

            self.advance(); // consume type (e.g. i64)

            def.fields[f_idx].name_len = core::cmp::min(f_name.len(), 16);
            def.fields[f_idx].name[..def.fields[f_idx].name_len].copy_from_slice(&f_name[..def.fields[f_idx].name_len]);
            def.fields[f_idx].offset = f_idx as u8;
            def.field_count += 1;

            if !self.match_token(TokenKind::Comma) {
                break;
            }
        }

        if !self.match_token(TokenKind::RBrace) {
            return Err(self.error(
                "ERR_EXPECTED_RBRACE",
                "Expected closing brace '}' after struct definition fields",
                "Right brace '}'",
                "Statement -> Struct Definition",
                "Close struct fields with '}'",
            ));
        }
        self.match_token(TokenKind::Semi);

        self.struct_defs[self.struct_def_count] = def;
        self.struct_def_count += 1;

        Ok(())
    }

    pub fn is_struct_type(&self, type_tok: Token) -> bool {
        let type_name = &self.src[type_tok.start..type_tok.start + type_tok.len];
        for i in 0..self.struct_def_count {
            let def = &self.struct_defs[i];
            if def.name_len == type_name.len() && &def.name[..def.name_len] == type_name {
                return true;
            }
        }
        false
    }

    pub fn declare_struct_inst(&mut self, var_tok: Token, type_tok: Token) -> Result<u8, CompileError> {
        let var_name = &self.src[var_tok.start..var_tok.start + var_tok.len];
        let type_name = &self.src[type_tok.start..type_tok.start + type_tok.len];

        let mut def_match = None;
        for i in 0..self.struct_def_count {
            let def = &self.struct_defs[i];
            if def.name_len == type_name.len() && &def.name[..def.name_len] == type_name {
                def_match = Some((i as u8, *def));
                break;
            }
        }

        let (def_idx, def) = match def_match {
            Some(d) => d,
            None => {
                return Err(self.error(
                    "ERR_UNKNOWN_STRUCT_TYPE",
                    "Struct type name is not defined",
                    "Declared struct type name",
                    "Statement -> Struct Instantiation",
                    "Declare struct before instantiating, e.g. 'struct Point { ... }'",
                ));
            }
        };

        if self.struct_inst_count >= 8 {
            return Err(self.error(
                "ERR_MAX_STRUCT_INSTS_EXCEEDED",
                "Maximum struct instances limit reached (8 struct instances limit)",
                "Fewer struct instances",
                "Struct Instantiation",
                "Reduce distinct struct instances across script",
            ));
        }

        let field_count = def.field_count as usize;
        if self.total_struct_fields + field_count > 256 {
            return Err(self.error(
                "ERR_STRUCT_CAPACITY_EXCEEDED",
                "Total static struct field capacity exceeded (max 256 fields)",
                "Fewer struct fields",
                "Struct Allocation",
                "Reduce number of fields across struct instances",
            ));
        }

        let inst_id = self.struct_inst_count as u8;
        let base_slot = self.total_struct_fields as u16;
        self.total_struct_fields += field_count;

        let mut inst = StructInstMeta::empty();
        inst.var_name_len = core::cmp::min(var_name.len(), 16);
        inst.var_name[..inst.var_name_len].copy_from_slice(&var_name[..inst.var_name_len]);
        inst.struct_def_idx = def_idx;
        inst.inst_id = inst_id;
        inst.base_slot = base_slot;
        inst.field_count = def.field_count;

        self.struct_insts[self.struct_inst_count] = inst;
        self.struct_inst_count += 1;

        self.emit_inst(PX64_OP_STRUCT_DEF, inst_id, def.field_count, 0)?;

        Ok(inst_id)
    }

    pub fn lookup_struct_field(&self, var_name: &[u8], field_name: &[u8]) -> Result<(u8, u8), CompileError> {
        let mut inst_match = None;
        for i in 0..self.struct_inst_count {
            let inst = &self.struct_insts[i];
            if inst.var_name_len == var_name.len() && &inst.var_name[..inst.var_name_len] == var_name {
                inst_match = Some(*inst);
                break;
            }
        }

        let inst = match inst_match {
            Some(i) => i,
            None => {
                return Err(self.error(
                    "ERR_UNKNOWN_STRUCT_VAR",
                    "Variable is not a declared struct instance",
                    "Struct variable",
                    "Expression -> Field Access",
                    "Declare struct instance with 'let $var: StructName;'",
                ));
            }
        };

        let def = &self.struct_defs[inst.struct_def_idx as usize];
        for f in 0..def.field_count as usize {
            let field = &def.fields[f];
            if field.name_len == field_name.len() && &field.name[..field.name_len] == field_name {
                return Ok((inst.inst_id, field.offset));
            }
        }

        Err(self.error(
            "ERR_UNKNOWN_STRUCT_FIELD",
            "Field does not exist on struct type",
            "Valid struct field name",
            "Expression -> Field Access",
            "Check field name in struct definition",
        ))
    }

    pub fn is_struct_inst(&self, var_name: &[u8]) -> bool {
        for i in 0..self.struct_inst_count {
            let inst = &self.struct_insts[i];
            if inst.var_name_len == var_name.len() && &inst.var_name[..inst.var_name_len] == var_name {
                return true;
            }
        }
        false
    }

    pub fn is_array(&self, tok: Token) -> bool {
        let name = &self.src[tok.start..tok.start + tok.len];
        for i in 0..self.array_count {
            let meta = &self.arrays[i];
            if meta.name_len == name.len() && &meta.name[..meta.name_len] == name {
                return true;
            }
        }
        false
    }

    fn count_array_literal_elements(&self) -> usize {
        let mut idx = self.current;
        let mut depth = 1;
        let mut count = 0;
        let mut has_elem = false;
        while idx < self.tokens.len() {
            match self.tokens[idx].kind {
                TokenKind::LBracket | TokenKind::LParen | TokenKind::LBrace => {
                    depth += 1;
                    has_elem = true;
                }
                TokenKind::RBracket | TokenKind::RParen | TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        if has_elem {
                            count += 1;
                        }
                        break;
                    }
                }
                TokenKind::Comma if depth == 1 => {
                    count += 1;
                    has_elem = false;
                }
                TokenKind::Eof => break,
                _ => {
                    has_elem = true;
                }
            }
            idx += 1;
        }
        count
    }

    pub fn declare_const_table(&mut self) -> Result<(), CompileError> {
        let name_tok = self.advance();
        if name_tok.kind != TokenKind::Ident {
            return Err(self.error(
                "ERR_EXPECTED_CONST_NAME",
                "Expected table identifier name after 'const' keyword",
                "Const table identifier name",
                "Statement -> Const Table Declaration",
                "Specify const table name, e.g. 'const LUT: [i64; 4] = [1, 2, 3, 4];'",
            ));
        }
        let t_name = &self.src[name_tok.start..name_tok.start + name_tok.len];
        if self.const_table_count >= 8 {
            return Err(self.error(
                "ERR_MAX_TABLES_EXCEEDED",
                "Maximum distinct const table limit reached (8 const tables limit)",
                "Fewer const tables",
                "Const Table Allocation",
                "Reduce distinct const tables across script",
            ));
        }

        for i in 0..self.const_table_count {
            let tbl = &self.const_tables[i];
            if tbl.name_len == t_name.len() && &tbl.name[..tbl.name_len] == t_name {
                return Err(self.error(
                    "ERR_TABLE_REDEFINED",
                    "Const table name is already defined",
                    "Unique const table name",
                    "Const Table Declaration",
                    "Rename duplicate const table",
                ));
            }
        }

        if !self.match_token(TokenKind::Colon) {
            return Err(self.error(
                "ERR_EXPECTED_COLON",
                "Expected ':' after const table name",
                "Colon ':'",
                "Statement -> Const Table Declaration",
                "Specify type annotation, e.g. 'const LUT: [i64; 4]'",
            ));
        }

        if !self.match_token(TokenKind::LBracket) {
            return Err(self.error(
                "ERR_EXPECTED_LBRACKET",
                "Expected '[' in const table type annotation",
                "Left bracket '['",
                "Statement -> Const Table Declaration",
                "Specify table type as '[i64; N]'",
            ));
        }

        self.advance(); // consume type (e.g. i64)

        if !self.match_token(TokenKind::Semi) {
            return Err(self.error(
                "ERR_TABLE_SYNTAX",
                "Expected ';' in table type [i64; N]",
                "Semicolon ';'",
                "Statement -> Const Table Declaration",
                "Specify table type as '[i64; N]'",
            ));
        }

        let len_tok = self.advance();
        let expected_len = match len_tok.kind {
            TokenKind::Number(n) if n > 0 && n <= 64 => n as usize,
            _ => {
                return Err(self.error(
                    "ERR_TABLE_INVALID_LEN",
                    "Const table size must be a positive constant integer (1..64)",
                    "Positive integer (1..64)",
                    "Statement -> Const Table Declaration",
                    "Specify valid table size, e.g. '[i64; 16]'",
                ))
            }
        };

        if !self.match_token(TokenKind::RBracket) {
            return Err(self.error(
                "ERR_TABLE_SYNTAX",
                "Expected ']' after table length",
                "Right bracket ']'",
                "Statement -> Const Table Declaration",
                "Close table type with ']'",
            ));
        }

        if !self.match_token(TokenKind::Eq) {
            return Err(self.error(
                "ERR_EXPECTED_EQ",
                "Expected '=' before table initialization array",
                "Equals sign '='",
                "Statement -> Const Table Declaration",
                "Assign initial values, e.g. '= [10, 20, 30]'",
            ));
        }

        if !self.match_token(TokenKind::LBracket) {
            return Err(self.error(
                "ERR_EXPECTED_LBRACKET",
                "Expected '[' to start table elements list",
                "Left bracket '['",
                "Statement -> Const Table Declaration",
                "Start elements list with '['",
            ));
        }

        if self.const_pool_len + expected_len > 256 {
            return Err(self.error(
                "ERR_CONST_POOL_EXCEEDED",
                "Constant pool capacity exceeded (max 256 constants)",
                "Fewer constants",
                "Constant Pool Allocation",
                "Reduce total constants and table elements across script",
            ));
        }

        let tbl_id = self.const_table_count as u8;
        let base_idx = self.const_pool_len as u8;
        let mut actual_len = 0;

        while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
            let mut is_neg = false;
            if self.peek().kind == TokenKind::Minus {
                self.advance();
                is_neg = true;
            }

            let elem_tok = self.advance();
            let val: i64 = match elem_tok.kind {
                TokenKind::Number(n) => {
                    if is_neg { -n } else { n }
                }
                TokenKind::TimeLiteral(ns) => ns as i64,
                _ => {
                    return Err(self.error(
                        "ERR_TABLE_NON_CONSTANT_ELEMENT",
                        "Const table elements must be compile-time constant integer or time literals",
                        "Constant literal value",
                        "Statement -> Const Table Declaration",
                        "Use constant literals for all table elements",
                    ));
                }
            };

            let _ = self.append_table_constant(val)?;
            actual_len += 1;

            if !self.match_token(TokenKind::Comma) {
                break;
            }
        }

        if actual_len != expected_len {
            return Err(self.error(
                "ERR_TABLE_LENGTH_MISMATCH",
                "Number of table elements does not match declared type length",
                "Matching number of elements",
                "Statement -> Const Table Declaration",
                "Ensure array element count matches declared length [i64; N]",
            ));
        }

        if !self.match_token(TokenKind::RBracket) {
            return Err(self.error(
                "ERR_EXPECTED_RBRACKET",
                "Expected closing bracket ']' after table elements list",
                "Right bracket ']'",
                "Statement -> Const Table Declaration",
                "Close elements list with ']'",
            ));
        }
        self.match_token(TokenKind::Semi);

        let mut meta = ConstTableMeta::empty();
        meta.name_len = core::cmp::min(t_name.len(), 16);
        meta.name[..meta.name_len].copy_from_slice(&t_name[..meta.name_len]);
        meta.tbl_id = tbl_id;
        meta.base_idx = base_idx;
        meta.len = expected_len as u8;

        self.const_tables[self.const_table_count] = meta;
        self.const_table_count += 1;

        self.emit_inst(PX64_OP_TBL_DEF, tbl_id, base_idx, expected_len as u8)?;

        Ok(())
    }

    pub fn lookup_const_table(&self, name: &[u8]) -> Result<(u8, u8), CompileError> {
        for i in 0..self.const_table_count {
            let meta = &self.const_tables[i];
            if meta.name_len == name.len() && &meta.name[..meta.name_len] == name {
                return Ok((meta.tbl_id, meta.len));
            }
        }
        Err(self.error(
            "ERR_UNKNOWN_CONST_TABLE",
            "Const table identifier is not defined",
            "Declared const table name",
            "Expression -> Const Table Access",
            "Define const table with 'const TABLE: [i64; N] = [...];'",
        ))
    }

    fn peek_ahead(&self, offset: usize) -> Token {
        if self.current + offset < self.tokens.len() {
            self.tokens[self.current + offset]
        } else {
            Token::empty()
        }
    }

    fn emit_const(&mut self, dst: u8, val: i64) -> Result<(), CompileError> {
        if (0..=65535).contains(&val) {
            self.emit_imm16(PX64_OP_MOV_IMM, dst, val as u16)?;
        } else {
            let idx = self.add_constant(val)?;
            self.emit_imm16(PX64_OP_LDC, dst, idx)?;
        }
        Ok(())
    }

    pub fn alloc_temp(&mut self) -> Result<u8, CompileError> {
        for i in 0..8 {
            let mask = 1u8 << i;
            if (self.temp_used & mask) == 0 {
                self.temp_used |= mask;
                return Ok(15 - i);
            }
        }
        Err(self.error(
            "ERR_EXPR_TOO_COMPLEX",
            "Expression nesting too deep (exceeded register scratch pool)",
            "Simpler expression or intermediate variables",
            "Expression Evaluation",
            "Split complex expression into intermediate variables",
        ))
    }

    pub fn free_temp(&mut self, reg: u8) {
        if (8..=15).contains(&reg) {
            let i = 15 - reg;
            self.temp_used &= !(1u8 << i);
        }
    }
    pub fn add_constant(&mut self, val: i64) -> Result<u16, CompileError> {
        for i in 0..self.const_pool_len {
            if self.const_pool[i] == val {
                return Ok(i as u16);
            }
        }
        if self.const_pool_len >= MAX_CONST_POOL {
            return Err(self.error(
                "ERR_CONST_POOL_FULL",
                "64-bit constant pool exhausted (64 entries limit reached)",
                "Fewer unique 64-bit constants",
                "Constant Pool Allocation",
                "Reduce large constant literals",
            ));
        }
        let idx = self.const_pool_len;
        self.const_pool[idx] = val;
        self.const_pool_len += 1;
        Ok(idx as u16)
    }

    pub fn append_table_constant(&mut self, val: i64) -> Result<u16, CompileError> {
        if self.const_pool_len >= MAX_CONST_POOL {
            return Err(self.error(
                "ERR_CONST_POOL_FULL",
                "64-bit constant pool exhausted (64 entries limit reached)",
                "Fewer unique 64-bit constants",
                "Constant Pool Allocation",
                "Reduce large constant literals or table size",
            ));
        }
        let idx = self.const_pool_len;
        self.const_pool[idx] = val;
        self.const_pool_len += 1;
        Ok(idx as u16)
    }

    fn peek(&self) -> Token {
        if self.current < self.tokens.len() {
            self.tokens[self.current]
        } else {
            Token::empty()
        }
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek();
        if self.current < self.tokens.len() {
            self.current += 1;
        }
        tok
    }

    fn match_token(&mut self, kind: TokenKind) -> bool {
        if self.peek().kind == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    fn error(
        &self,
        code: &'static str,
        message: &'static str,
        expected: &'static str,
        stage: &'static str,
        suggestion: &'static str,
    ) -> CompileError {
        let tok = self.peek();
        CompileError {
            code,
            message,
            line: tok.line,
            col: tok.col,
            byte_offset: tok.start,
            token_kind: tok.kind,
            token_len: tok.len,
            expected,
            stage,
            suggestion,
        }
    }

    fn emit_inst(&mut self, op: u8, rd: u8, rs1: u8, rs2: u8) -> Result<usize, CompileError> {
        if self.code_len + 4 > MAX_BYTECODE_SIZE {
            return Err(self.error(
                "ERR_BYTECODE_OVERFLOW",
                "px64 bytecode buffer overflow (1024 bytes limit reached)",
                "Smaller script size or simplify expressions",
                "Code Generation",
                "Reduce script length or complexity",
            ));
        }
        let pos = self.code_len;
        self.code[pos] = op;
        self.code[pos + 1] = rd;
        self.code[pos + 2] = rs1;
        self.code[pos + 3] = rs2;
        self.code_len += 4;
        Ok(pos)
    }

    fn emit_imm16(&mut self, op: u8, rd: u8, imm: u16) -> Result<usize, CompileError> {
        self.emit_inst(op, rd, (imm >> 8) as u8, (imm & 0xFF) as u8)
    }

    fn patch_imm16(&mut self, pos: usize, imm: u16) {
        if pos + 3 < MAX_BYTECODE_SIZE {
            self.code[pos + 2] = (imm >> 8) as u8;
            self.code[pos + 3] = (imm & 0xFF) as u8;
        }
    }

    fn declare_var(&mut self, tok: Token, is_mut: bool) -> Result<u8, CompileError> {
        let name = &self.src[tok.start..tok.start + tok.len];
        match name {
            b"$rax" | b"$r0" => return Ok(0),
            b"$rcx" | b"$r1" => return Ok(1),
            b"$rdx" | b"$r2" => return Ok(2),
            b"$rbx" | b"$r3" => return Ok(3),
            b"$rsp" | b"$r4" => return Ok(4),
            b"$rbp" | b"$r5" => return Ok(5),
            b"$rsi" | b"$r6" => return Ok(6),
            b"$rdi" | b"$r7" => return Ok(7),
            b"$r8" => return Ok(8),
            b"$r9" => return Ok(9),
            b"$r10" => return Ok(10),
            b"$r11" => return Ok(11),
            b"$r12" => return Ok(12),
            b"$r13" => return Ok(13),
            b"$r14" => return Ok(14),
            b"$r15" => return Ok(15),
            b"#f" | b"#frame" | b"#f0" | b"#slot0" => return Ok(16),
            b"#f1" | b"#slot1" => return Ok(17),
            b"#f2" | b"#slot2" => return Ok(18),
            b"#f3" | b"#slot3" => return Ok(19),
            _ => {}
        }

        for i in 0..self.var_count {
            if self.var_lens[i] == name.len() && &self.var_names[i][..self.var_lens[i]] == name {
                self.var_mut[i] = is_mut;
                return Ok(self.var_regs[i]);
            }
        }

        if self.var_count >= 13 {
            return Err(self.error(
                "ERR_MAX_VARS_EXCEEDED",
                "Maximum distinct variables limit reached (13 general-purpose registers)",
                "Reuse existing $variables or registers ($rax, $rcx, etc.)",
                "Register Allocation",
                "Reduce distinct variable count in script",
            ));
        }

        let reg = (1 + self.var_count) as u8;
        let idx = self.var_count;
        let len = core::cmp::min(name.len(), 16);
        self.var_names[idx][..len].copy_from_slice(&name[..len]);
        self.var_lens[idx] = len;
        self.var_regs[idx] = reg;
        self.var_mut[idx] = is_mut;
        self.var_count += 1;
        Ok(reg)
    }

    fn check_var_mutation(&self, tok: Token) -> Result<(), CompileError> {
        let name = &self.src[tok.start..tok.start + tok.len];
        if name.starts_with(b"#")
            || name == b"$rax"
            || name == b"$rcx"
            || name == b"$rdx"
            || name == b"$rbx"
            || name == b"$rsp"
            || name == b"$rbp"
            || name == b"$rsi"
            || name == b"$rdi"
            || name.starts_with(b"$r")
        {
            return Ok(());
        }

        for i in 0..self.var_count {
            if self.var_lens[i] == name.len() && &self.var_names[i][..self.var_lens[i]] == name {
                if !self.var_mut[i] {
                    return Err(self.error(
                        "ERR_MUTABILITY_VIOLATION",
                        "Cannot reassign/mutate immutable variable; variables are immutable by default in PulseLang",
                        "Mutable variable declaration ('let mut')",
                        "Statement -> Variable Assignment",
                        "Declare variable with 'let mut $var = ...;' to permit mutation",
                    ));
                }
                return Ok(());
            }
        }
        Ok(())
    }

    fn resolve_var(&mut self, tok: Token) -> Result<u8, CompileError> {
        let name = &self.src[tok.start..tok.start + tok.len];
        match name {
            b"$rax" | b"$r0" => return Ok(0),
            b"$rcx" | b"$r1" => return Ok(1),
            b"$rdx" | b"$r2" => return Ok(2),
            b"$rbx" | b"$r3" => return Ok(3),
            b"$rsp" | b"$r4" => return Ok(4),
            b"$rbp" | b"$r5" => return Ok(5),
            b"$rsi" | b"$r6" => return Ok(6),
            b"$rdi" | b"$r7" => return Ok(7),
            b"$r8" => return Ok(8),
            b"$r9" => return Ok(9),
            b"$r10" => return Ok(10),
            b"$r11" => return Ok(11),
            b"$r12" => return Ok(12),
            b"$r13" => return Ok(13),
            b"$r14" => return Ok(14),
            b"$r15" => return Ok(15),
            b"#f" | b"#frame" | b"#f0" | b"#slot0" => return Ok(16),
            b"#f1" | b"#slot1" => return Ok(17),
            b"#f2" | b"#slot2" => return Ok(18),
            b"#f3" | b"#slot3" => return Ok(19),
            _ => {}
        }

        // Check if variable is a parameter of current function
        if let Some(fn_idx) = self.current_fn {
            let meta = &self.functions[fn_idx];
            let param_regs: [u8; 4] = [7, 6, 2, 1]; // $rdi, $rsi, $rdx, $rcx
            for p in 0..meta.param_count as usize {
                if meta.param_lens[p] == name.len() && &meta.param_names[p][..meta.param_lens[p]] == name {
                    return Ok(param_regs[p]);
                }
            }
        }

        for i in 0..self.var_count {
            if self.var_lens[i] == name.len() && &self.var_names[i][..self.var_lens[i]] == name {
                return Ok(self.var_regs[i]);
            }
        }

        if self.var_count >= 13 {
            return Err(self.error(
                "ERR_MAX_VARS_EXCEEDED",
                "Maximum distinct variables limit reached (13 general-purpose registers)",
                "Reuse existing $variables or registers ($rax, $rcx, etc.)",
                "Register Allocation",
                "Reduce distinct variable count in script",
            ));
        }

        let reg = (1 + self.var_count) as u8;
        let idx = self.var_count;
        let len = core::cmp::min(name.len(), 16);
        self.var_names[idx][..len].copy_from_slice(&name[..len]);
        self.var_lens[idx] = len;
        self.var_regs[idx] = reg;
        self.var_mut[idx] = true;
        self.var_count += 1;
        Ok(reg)
    }

    /// Single-pass compilation from tokens into px64 fixed 32-bit instructions.
    pub fn compile(&mut self) -> Result<usize, CompileError> {
        while self.peek().kind != TokenKind::Eof {
            self.statement()?;
        }

        // Verify all allocated hardware handles were sent/consumed (linear type ownership)
        for i in 0..4 {
            if let HandleState::Allocated { line, col } = self.handle_states[i] {
                return Err(CompileError {
                    code: "ERR_LINEAR_UNCONSUMED_HANDLE",
                    message: "Hardware handle captured but never sent/consumed (linear ownership violation)",
                    line,
                    col,
                    byte_offset: 0,
                    token_kind: TokenKind::HardwareIdent,
                    token_len: 2,
                    expected: "Consume handle with @send(#handle)",
                    stage: "Linear Ownership Verification",
                    suggestion: "Add '@send(#handle);' along all execution paths to release DMA buffer",
                });
            }
        }

        self.emit_inst(PX64_OP_HALT, 0, 0, 0)?;
        Ok(self.code_len)
    }

    fn statement(&mut self) -> Result<(), CompileError> {
        let tok = self.peek();

        match tok.kind {
            TokenKind::Semi => {
                self.advance();
                return Ok(());
            }
            TokenKind::AtContract => {
                self.advance();
                self.match_token(TokenKind::Colon);
                while self.peek().kind != TokenKind::Semi && self.peek().kind != TokenKind::Eof {
                    self.advance();
                }
                if !self.match_token(TokenKind::Semi) {
                    return Err(self.error(
                        "ERR_MISSING_SEMICOLON",
                        "Missing semicolon ';' after @contract directive",
                        "Semicolon ';'",
                        "Statement -> @contract Directive",
                        "Add ';' at end of @contract: ...;",
                    ));
                }
            }

            TokenKind::AtPipeline | TokenKind::Pipeline => {
                self.advance();
                if self.peek().kind == TokenKind::Colon {
                    self.advance();
                }
                let _name = self.advance();
                if self.peek().kind == TokenKind::AtBudget || self.peek().kind == TokenKind::Budget {
                    self.advance();
                    if self.match_token(TokenKind::LParen) {
                        self.advance();
                        self.match_token(TokenKind::RParen);
                    }
                }
                if self.match_token(TokenKind::Semi) {
                    return Ok(());
                }
                if self.match_token(TokenKind::LBrace) {
                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        self.statement()?;
                    }
                    self.match_token(TokenKind::RBrace);
                }
            }

            TokenKind::AtOnVblank | TokenKind::On => {
                self.advance();
                if self.peek().kind == TokenKind::Colon {
                    self.advance();
                }
                if self.match_token(TokenKind::LParen) {
                    self.advance();
                    self.match_token(TokenKind::RParen);
                }
                if self.match_token(TokenKind::LBrace) {
                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        self.statement()?;
                    }
                    self.match_token(TokenKind::RBrace);
                    self.match_token(TokenKind::Semi);
                }
            }

            TokenKind::AtWithin | TokenKind::Within => {
                self.advance();
                let has_paren = self.match_token(TokenKind::LParen);
                let time_tok = self.advance();
                if has_paren {
                    self.match_token(TokenKind::RParen);
                }

                let deadline_ns = match time_tok.kind {
                    TokenKind::TimeLiteral(ns) => ns,
                    TokenKind::Number(n) => n as u64,
                    _ => {
                        return Err(self.error(
                            "ERR_EXPECTED_TIME_LITERAL",
                            "Expected time duration literal after @within (e.g. 500us, 10ms, 100ns)",
                            "Time literal with unit suffix (e.g. 500us, 5ms, 100ns)",
                            "Statement -> @within Block",
                            "Specify duration like @within(500us) { ... }",
                        ));
                    }
                };

                let time_reg = 0;
                let val = deadline_ns as i64;
                if (0..=65535).contains(&val) {
                    self.emit_imm16(PX64_OP_MOV_IMM, time_reg, val as u16)?;
                } else {
                    let idx = self.add_constant(val)?;
                    self.emit_imm16(PX64_OP_LDC, time_reg, idx)?;
                }
                self.emit_inst(PX64_OP_WITHIN_START, time_reg, 0, 0)?;

                if !self.match_token(TokenKind::LBrace) {
                    return Err(self.error(
                        "ERR_EXPECTED_LBRACE",
                        "Missing opening brace '{' to begin within block",
                        "Left brace '{'",
                        "Statement -> @within Block",
                        "Add opening brace '{' after @within(...)",
                    ));
                }
                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    self.statement()?;
                }
                self.match_token(TokenKind::RBrace);
                self.emit_inst(PX64_OP_WITHIN_END, 0, 0, 0)?;

                if self.match_token(TokenKind::Exclamation) || self.match_token(TokenKind::Or) {
                    if self.match_token(TokenKind::Drop) {
                        self.emit_inst(PX64_OP_DROP, 0, 0, 0)?;
                    }
                }
                self.match_token(TokenKind::Semi);
            }

            TokenKind::AtWhile | TokenKind::While => {
                self.advance();
                let loop_start = self.code_len as u16;
                self.match_token(TokenKind::LParen);

                // Static Loop Boundary Verification
                let cond_tok = self.peek();
                if let TokenKind::Number(n) = cond_tok.kind {
                    if n != 0 {
                        return Err(self.error(
                            "ERR_UNBOUNDED_LOOP",
                            "Constant infinite loop (@while(1)) rejected: loops must have statically bounded monotonic conditions",
                            "Bounded condition e.g. @while($i < 100)",
                            "Static Loop Bound Verification",
                            "Replace infinite loop with bounded loop counter",
                        ));
                    }
                }

                let cond_reg = 0;
                self.expression(cond_reg)?;
                self.match_token(TokenKind::RParen);

                let jz_pos = self.emit_inst(PX64_OP_JZ, cond_reg, 0, 0)?;

                if !self.match_token(TokenKind::LBrace) {
                    return Err(self.error(
                        "ERR_EXPECTED_LBRACE",
                        "Missing opening brace '{' for while block",
                        "Left brace '{'",
                        "Statement -> While Loop",
                        "Add opening brace '{' after while condition",
                    ));
                }
                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    self.statement()?;
                }
                self.match_token(TokenKind::RBrace);

                self.emit_imm16(PX64_OP_JMP, 0, loop_start)?;
                self.patch_imm16(jz_pos, self.code_len as u16);
            }

            TokenKind::For | TokenKind::AtFor => {
                self.advance();
                let has_paren = self.match_token(TokenKind::LParen);

                // 1. Loop variable: must be a VarIdent ($i, $idx, etc.)
                let var_tok = self.peek();
                if var_tok.kind != TokenKind::VarIdent {
                    return Err(self.error(
                        "ERR_FOR_EXPECTED_VAR",
                        "Expected variable identifier (e.g., $i) after 'for'",
                        "Variable identifier '$var'",
                        "Statement -> For Loop",
                        "Specify loop variable, e.g., 'for $i in 0..10'",
                    ));
                }
                let var_tok = self.advance();
                let var_reg = self.resolve_var(var_tok)?;

                // 2. 'in' keyword
                if !self.match_token(TokenKind::In) {
                    return Err(self.error(
                        "ERR_FOR_EXPECTED_IN",
                        "Expected 'in' keyword after loop variable",
                        "'in'",
                        "Statement -> For Loop",
                        "Add 'in' keyword after variable, e.g., 'for $i in 0..10'",
                    ));
                }

                // 3. Start bound: Must be compile-time constant integer or time literal
                let start_tok = self.peek();
                let start_val: i64 = match start_tok.kind {
                    TokenKind::Number(n) => {
                        self.advance();
                        n
                    }
                    TokenKind::TimeLiteral(ns) => {
                        self.advance();
                        ns as i64
                    }
                    _ => {
                        return Err(self.error(
                            "ERR_FOR_NON_CONSTANT_BOUND",
                            "Static range for loop requires compile-time constant start bound (0..N)",
                            "Integer literal e.g. '0'",
                            "Statement -> For Loop",
                            "Use constant literal for loop range start, or use 'while' for dynamic bounds",
                        ));
                    }
                };

                // 4. '..' range operator
                if !self.match_token(TokenKind::DotDot) {
                    return Err(self.error(
                        "ERR_FOR_EXPECTED_DOTDOT",
                        "Expected range operator '..' between loop bounds",
                        "'..'",
                        "Statement -> For Loop",
                        "Specify range with '..', e.g., '0..100'",
                    ));
                }

                // 5. End bound: Must be compile-time constant integer or time literal
                let end_tok = self.peek();
                let end_val: i64 = match end_tok.kind {
                    TokenKind::Number(n) => {
                        self.advance();
                        n
                    }
                    TokenKind::TimeLiteral(ns) => {
                        self.advance();
                        ns as i64
                    }
                    _ => {
                        return Err(self.error(
                            "ERR_FOR_NON_CONSTANT_BOUND",
                            "Static range for loop requires compile-time constant end bound (0..N)",
                            "Integer literal e.g. '100'",
                            "Statement -> For Loop",
                            "Use constant literal for loop range end, or use 'while' for dynamic bounds",
                        ));
                    }
                };

                if has_paren {
                    self.match_token(TokenKind::RParen);
                }

                // Calculate iterations
                let iterations: u64 = if end_val > start_val {
                    (end_val - start_val) as u64
                } else {
                    0
                };

                // Emit start initialization: $i = start_val
                if (0..=65535).contains(&start_val) {
                    self.emit_imm16(PX64_OP_MOV_IMM, var_reg, start_val as u16)?;
                } else {
                    let idx = self.add_constant(start_val)?;
                    self.emit_imm16(PX64_OP_LDC, var_reg, idx)?;
                }

                // Emit end value into a temp register
                let end_reg = self.alloc_temp()?;
                if (0..=65535).contains(&end_val) {
                    self.emit_imm16(PX64_OP_MOV_IMM, end_reg, end_val as u16)?;
                } else {
                    let idx = self.add_constant(end_val)?;
                    self.emit_imm16(PX64_OP_LDC, end_reg, idx)?;
                }

                let loop_start = self.code_len as u16;

                // Condition: $rax = ($i < end_reg)
                self.emit_inst(PX64_OP_CMP_LT, 0, var_reg, end_reg)?;
                let jz_pos = self.emit_inst(PX64_OP_JZ, 0, 0, 0)?;

                // Parse loop body
                if !self.match_token(TokenKind::LBrace) {
                    return Err(self.error(
                        "ERR_EXPECTED_LBRACE",
                        "Missing opening brace '{' for for loop body",
                        "Left brace '{'",
                        "Statement -> For Loop",
                        "Add opening brace '{' after for loop range",
                    ));
                }

                let body_code_start = self.code_len;
                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    self.statement()?;
                }
                self.match_token(TokenKind::RBrace);
                let body_code_end = self.code_len;

                // Static WCET validation:
                // Body instructions = (body_code_end - body_code_start) / 4
                // Loop overhead per iteration: CMPLT (1) + JZ (1) + ADDI (1) + JMP (1) = 4 instructions
                let body_inst_count = (body_code_end - body_code_start) / 4;
                let insts_per_iter = body_inst_count + 4;
                let total_estimated_steps = (insts_per_iter as u64).saturating_mul(iterations);

                if total_estimated_steps > MAX_VM_STEPS as u64 {
                    return Err(self.error(
                        "ERR_FOR_WCET_EXCEEDED",
                        "Static loop WCET exceeds MAX_VM_STEPS step limit (10,000 steps)",
                        "Loop with total steps <= 10,000",
                        "Static Loop WCET Verification",
                        "Reduce loop upper bound or simplify loop body to satisfy real-time step budget",
                    ));
                }

                // Increment: $i += 1
                self.emit_inst(PX64_OP_ADDI, var_reg, var_reg, 1)?;

                // Jump back to loop start
                self.emit_imm16(PX64_OP_JMP, 0, loop_start)?;

                // Patch exit jump
                self.patch_imm16(jz_pos, self.code_len as u16);

                // Free temp register
                self.free_temp(end_reg);
            }

            TokenKind::Let => {
                self.advance();
                let is_mut = self.match_token(TokenKind::Mut);
                let ident = self.advance();
                if self.match_token(TokenKind::Colon) {
                    // Array declaration: let $buf: [i64; N]; or let $buf: [i64; N] = [1, 2, 3];
                    if self.match_token(TokenKind::LBracket) {
                        let _type_tok = self.advance(); // i64 or Ident
                        if !self.match_token(TokenKind::Semi) {
                            return Err(self.error(
                                "ERR_ARRAY_SYNTAX",
                                "Expected ';' in array type [i64; N]",
                                "Semicolon ';'",
                                "Statement -> Array Declaration",
                                "Specify array type as '[i64; N]'",
                            ));
                        }
                        let len_tok = self.advance();
                        let len = match len_tok.kind {
                            TokenKind::Number(n) if n > 0 => n as usize,
                            _ => {
                                return Err(self.error(
                                    "ERR_ARRAY_INVALID_LEN",
                                    "Array size must be a positive constant integer",
                                    "Positive integer",
                                    "Statement -> Array Declaration",
                                    "Specify constant array size, e.g. '[i64; 16]'",
                                ))
                            }
                        };
                        if !self.match_token(TokenKind::RBracket) {
                            return Err(self.error(
                                "ERR_ARRAY_SYNTAX",
                                "Expected ']' after array length",
                                "Right bracket ']'",
                                "Statement -> Array Declaration",
                                "Close array type with ']'",
                            ));
                        }

                        let arr_id = self.declare_array(ident, len)?;
                        self.emit_imm16(PX64_OP_ARR_DEF, arr_id, len as u16)?;

                        // Optional initialization: = [elem0, elem1, ...] or := [elem0, elem1, ...]
                        if self.match_token(TokenKind::ColonEq) || self.match_token(TokenKind::Eq) {
                            if !self.match_token(TokenKind::LBracket) {
                                return Err(self.error(
                                    "ERR_EXPECTED_LBRACKET",
                                    "Expected '[' to start array literal initialization",
                                    "Left bracket '['",
                                    "Statement -> Array Initialization",
                                    "Initialize array elements with '= [elem0, elem1, ...]'",
                                ));
                            }
                            let mut elem_idx = 0;
                            while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
                                if elem_idx >= len {
                                    return Err(self.error(
                                        "ERR_ARRAY_INIT_TOO_MANY_ELEMENTS",
                                        "Array literal has more elements than declared array capacity",
                                        "Matching number of elements",
                                        "Statement -> Array Initialization",
                                        "Ensure element count does not exceed declared array size",
                                    ));
                                }
                                let idx_reg = self.alloc_temp()?;
                                let val_reg = self.alloc_temp()?;
                                self.emit_inst(PX64_OP_MOV_IMM, idx_reg, (elem_idx >> 8) as u8, (elem_idx & 0xff) as u8)?;
                                self.expression(val_reg)?;
                                self.emit_inst(PX64_OP_ARR_STORE, arr_id, idx_reg, val_reg)?;
                                self.free_temp(val_reg);
                                self.free_temp(idx_reg);
                                elem_idx += 1;
                                if !self.match_token(TokenKind::Comma) {
                                    break;
                                }
                            }
                            if !self.match_token(TokenKind::RBracket) {
                                return Err(self.error(
                                    "ERR_ARRAY_SYNTAX",
                                    "Expected ']' after array initialization elements",
                                    "Right bracket ']'",
                                    "Statement -> Array Initialization",
                                    "Close array initialization list with ']'",
                                ));
                            }
                        }

                        self.match_token(TokenKind::Semi);
                        return Ok(());
                    } else {
                        // Struct instance declaration: let $pt: Point; or type annotation let $x: i64 = 10;
                        let type_tok = self.advance();
                        if self.is_struct_type(type_tok) {
                            self.declare_struct_inst(ident, type_tok)?;
                            self.match_token(TokenKind::Semi);
                            return Ok(());
                        }
                        // Primitive type annotation, e.g. let $x: i64 = 10;
                    }
                }

                // Check for array literal initialization without type annotation:
                // e.g. let $a = [1, 2, 3]; or let mut $a = [1, 2, 3];
                if (self.peek().kind == TokenKind::ColonEq || self.peek().kind == TokenKind::Eq)
                    && self.peek_ahead(1).kind == TokenKind::LBracket
                {
                    self.advance(); // consume '=' or ':='
                    self.advance(); // consume '['

                    let len = self.count_array_literal_elements();
                    if len == 0 {
                        return Err(self.error(
                            "ERR_ARRAY_INVALID_LEN",
                            "Array literal cannot be empty",
                            "At least one element",
                            "Statement -> Array Initialization",
                            "Provide elements in array literal, e.g. '[1, 2, 3]'",
                        ));
                    }
                    let arr_id = self.declare_array(ident, len)?;
                    self.emit_imm16(PX64_OP_ARR_DEF, arr_id, len as u16)?;

                    let mut elem_idx = 0;
                    while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
                        let idx_reg = self.alloc_temp()?;
                        let val_reg = self.alloc_temp()?;
                        self.emit_inst(PX64_OP_MOV_IMM, idx_reg, (elem_idx >> 8) as u8, (elem_idx & 0xff) as u8)?;
                        self.expression(val_reg)?;
                        self.emit_inst(PX64_OP_ARR_STORE, arr_id, idx_reg, val_reg)?;
                        self.free_temp(val_reg);
                        self.free_temp(idx_reg);
                        elem_idx += 1;
                        if !self.match_token(TokenKind::Comma) {
                            break;
                        }
                    }
                    if !self.match_token(TokenKind::RBracket) {
                        return Err(self.error(
                            "ERR_ARRAY_SYNTAX",
                            "Expected ']' after array initialization elements",
                            "Right bracket ']'",
                            "Statement -> Array Initialization",
                            "Close array initialization list with ']'",
                        ));
                    }
                    self.match_token(TokenKind::Semi);
                    return Ok(());
                }
                // Check for view declaration: e.g. let $row = @row($a, $i, 3);
                if (self.peek().kind == TokenKind::ColonEq || self.peek().kind == TokenKind::Eq)
                    && (self.peek_ahead(1).kind == TokenKind::IntrinsicIdent || self.peek_ahead(1).kind == TokenKind::Ident)
                {
                    let next_tok = self.peek_ahead(1);
                    let name = &self.src[next_tok.start..next_tok.start + next_tok.len];
                    if name == b"@row" || name == b"@col" || name == b"@slice" {
                        self.advance(); // consume '=' or ':='
                        let view = self.parse_view_source()?;
                        self.declare_view(ident, view.arr_id, view.base_reg, view.stride_imm, view.len_imm)?;
                        self.match_token(TokenKind::Semi);
                        return Ok(());
                    }
                }

                let var_reg = self.declare_var(ident, is_mut)?;
                if self.match_token(TokenKind::ColonEq) || self.match_token(TokenKind::Eq) {
                    self.expression(var_reg)?;
                }
                self.match_token(TokenKind::Semi);
            }

            TokenKind::AtAssert => {
                self.advance();
                self.match_token(TokenKind::LParen);
                let cond_reg = 0;
                self.expression(cond_reg)?;
                self.match_token(TokenKind::RParen);
                self.match_token(TokenKind::Semi);
                self.emit_inst(PX64_OP_ASSERT, cond_reg, 0, 0)?;
            }

            TokenKind::If => {
                self.advance();
                self.match_token(TokenKind::LParen);
                let cond_reg = 0;
                self.expression(cond_reg)?;
                self.match_token(TokenKind::RParen);

                let jz_pos = self.emit_inst(PX64_OP_JZ, cond_reg, 0, 0)?;

                if !self.match_token(TokenKind::LBrace) {
                    return Err(self.error(
                        "ERR_EXPECTED_LBRACE",
                        "Missing opening brace '{' after if condition",
                        "Left brace '{'",
                        "Statement -> If Branch",
                        "Add opening brace '{' after if (...) condition",
                    ));
                }
                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    self.statement()?;
                }
                self.match_token(TokenKind::RBrace);

                if self.match_token(TokenKind::Else) {
                    let jmp_pos = self.emit_inst(PX64_OP_JMP, 0, 0, 0)?;
                    self.patch_imm16(jz_pos, self.code_len as u16);

                    if !self.match_token(TokenKind::LBrace) {
                        return Err(self.error(
                            "ERR_EXPECTED_LBRACE",
                            "Missing opening brace '{' after else keyword",
                            "Left brace '{'",
                            "Statement -> Else Branch",
                            "Add opening brace '{' after else",
                        ));
                    }
                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        self.statement()?;
                    }
                    self.match_token(TokenKind::RBrace);
                    self.patch_imm16(jmp_pos, self.code_len as u16);
                } else {
                    self.patch_imm16(jz_pos, self.code_len as u16);
                }
            }

            TokenKind::Match => {
                self.advance(); // consume 'match'
                let match_reg = self.alloc_temp()?;
                self.expression(match_reg)?;
                if !self.match_token(TokenKind::LBrace) {
                    return Err(self.error(
                        "ERR_EXPECTED_LBRACE",
                        "Missing opening brace '{' after match expression",
                        "Left brace '{'",
                        "Statement -> Match Block",
                        "Add '{' to open match arms",
                    ));
                }

                let mut arm_jmp_ends = [0usize; 16];
                let mut arm_count = 0;
                let mut has_ok_arm = false;
                let mut has_err_arm = false;
                let mut has_wildcard = false;

                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    if arm_count >= 16 {
                        return Err(self.error(
                            "ERR_MAX_MATCH_ARMS_EXCEEDED",
                            "Maximum match arms exceeded (up to 16 arms supported)",
                            "<= 16 arms",
                            "Statement -> Match Arm",
                            "Reduce the number of match arms",
                        ));
                    }

                    let pat_tok = self.peek();
                    let name = &self.src[pat_tok.start..pat_tok.start + pat_tok.len];

                    if pat_tok.kind == TokenKind::Underscore || name == b"_" {
                        self.advance(); // consume '_'
                        has_wildcard = true;
                        if !self.match_token(TokenKind::FatArrow) {
                            return Err(self.error(
                                "ERR_EXPECTED_FAT_ARROW",
                                "Expected '=>' after match pattern",
                                "Fat arrow '=>'",
                                "Statement -> Match Arm",
                                "Specify arm as 'pattern => { ... },'",
                            ));
                        }

                        if self.match_token(TokenKind::LBrace) {
                            while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                                self.statement()?;
                            }
                            self.match_token(TokenKind::RBrace);
                        } else {
                            self.statement()?;
                        }
                        self.match_token(TokenKind::Comma);
                        let jmp_end = self.emit_imm16(PX64_OP_JMP, 0, 0)?;
                        arm_jmp_ends[arm_count] = jmp_end;
                        arm_count += 1;
                        break;
                    } else if name == b"Ok" {
                        self.advance(); // consume 'Ok'
                        has_ok_arm = true;
                        if !self.match_token(TokenKind::LParen) {
                            return Err(self.error(
                                "ERR_EXPECTED_LPAREN",
                                "Expected '(' in Ok($var) pattern",
                                "Left parenthesis '('",
                                "Match Pattern -> Ok Variant",
                                "Specify as 'Ok($val) => { ... }'",
                            ));
                        }
                        let bind_tok = self.advance();
                        if !self.match_token(TokenKind::RParen) {
                            return Err(self.error(
                                "ERR_UNCLOSED_PAREN",
                                "Expected ')' after Ok pattern binding variable",
                                "Right parenthesis ')'",
                                "Match Pattern -> Ok Variant",
                                "Close Ok pattern with ')'",
                            ));
                        }
                        if !self.match_token(TokenKind::FatArrow) {
                            return Err(self.error(
                                "ERR_EXPECTED_FAT_ARROW",
                                "Expected '=>' after match pattern",
                                "Fat arrow '=>'",
                                "Statement -> Match Arm",
                                "Specify arm as 'Ok($var) => { ... },'",
                            ));
                        }

                        let test_reg = self.alloc_temp()?;
                        self.emit_inst(PX64_OP_CALL_NAT, test_reg, NATIVE_IS_ERR, match_reg)?;
                        let jnz_skip = self.emit_imm16(PX64_OP_JNZ, test_reg, 0)?;
                        self.free_temp(test_reg);

                        let bind_reg = self.resolve_var(bind_tok)?;
                        let unwrap_reg = self.alloc_temp()?;
                        self.emit_inst(PX64_OP_CALL_NAT, unwrap_reg, NATIVE_UNWRAP, match_reg)?;
                        self.emit_inst(PX64_OP_MOV_REG, bind_reg, unwrap_reg, 0)?;
                        self.free_temp(unwrap_reg);

                        if self.match_token(TokenKind::LBrace) {
                            while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                                self.statement()?;
                            }
                            self.match_token(TokenKind::RBrace);
                        } else {
                            self.statement()?;
                        }
                        self.match_token(TokenKind::Comma);

                        let jmp_end = self.emit_imm16(PX64_OP_JMP, 0, 0)?;
                        arm_jmp_ends[arm_count] = jmp_end;
                        arm_count += 1;

                        self.patch_imm16(jnz_skip, self.code_len as u16);
                    } else if name == b"Err" {
                        self.advance(); // consume 'Err'
                        has_err_arm = true;
                        if !self.match_token(TokenKind::LParen) {
                            return Err(self.error(
                                "ERR_EXPECTED_LPAREN",
                                "Expected '(' in Err($var) pattern",
                                "Left parenthesis '('",
                                "Match Pattern -> Err Variant",
                                "Specify as 'Err($code) => { ... }'",
                            ));
                        }
                        let bind_tok = self.advance();
                        if !self.match_token(TokenKind::RParen) {
                            return Err(self.error(
                                "ERR_UNCLOSED_PAREN",
                                "Expected ')' after Err pattern binding variable",
                                "Right parenthesis ')'",
                                "Match Pattern -> Err Variant",
                                "Close Err pattern with ')'",
                            ));
                        }
                        if !self.match_token(TokenKind::FatArrow) {
                            return Err(self.error(
                                "ERR_EXPECTED_FAT_ARROW",
                                "Expected '=>' after match pattern",
                                "Fat arrow '=>'",
                                "Statement -> Match Arm",
                                "Specify arm as 'Err($var) => { ... },'",
                            ));
                        }

                        let test_reg = self.alloc_temp()?;
                        self.emit_inst(PX64_OP_CALL_NAT, test_reg, NATIVE_IS_OK, match_reg)?;
                        let jnz_skip = self.emit_imm16(PX64_OP_JNZ, test_reg, 0)?;
                        self.free_temp(test_reg);

                        let bind_reg = self.resolve_var(bind_tok)?;
                        let tag_reg = self.alloc_temp()?;
                        self.emit_const(tag_reg, ERR_TAG)?;
                        self.emit_inst(PX64_OP_XOR, bind_reg, match_reg, tag_reg)?;
                        self.free_temp(tag_reg);

                        if self.match_token(TokenKind::LBrace) {
                            while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                                self.statement()?;
                            }
                            self.match_token(TokenKind::RBrace);
                        } else {
                            self.statement()?;
                        }
                        self.match_token(TokenKind::Comma);

                        let jmp_end = self.emit_imm16(PX64_OP_JMP, 0, 0)?;
                        arm_jmp_ends[arm_count] = jmp_end;
                        arm_count += 1;

                        self.patch_imm16(jnz_skip, self.code_len as u16);
                    } else {
                        let pat_reg = self.alloc_temp()?;
                        self.expression(pat_reg)?;
                        if !self.match_token(TokenKind::FatArrow) {
                            return Err(self.error(
                                "ERR_EXPECTED_FAT_ARROW",
                                "Expected '=>' after match pattern",
                                "Fat arrow '=>'",
                                "Statement -> Match Arm",
                                "Specify arm as 'value => { ... },'",
                            ));
                        }

                        let cmp_reg = self.alloc_temp()?;
                        self.emit_inst(PX64_OP_CMP_EQ, cmp_reg, match_reg, pat_reg)?;
                        self.free_temp(pat_reg);
                        let jz_skip = self.emit_imm16(PX64_OP_JZ, cmp_reg, 0)?;
                        self.free_temp(cmp_reg);

                        if self.match_token(TokenKind::LBrace) {
                            while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                                self.statement()?;
                            }
                            self.match_token(TokenKind::RBrace);
                        } else {
                            self.statement()?;
                        }
                        self.match_token(TokenKind::Comma);

                        let jmp_end = self.emit_imm16(PX64_OP_JMP, 0, 0)?;
                        arm_jmp_ends[arm_count] = jmp_end;
                        arm_count += 1;

                        self.patch_imm16(jz_skip, self.code_len as u16);
                    }
                }

                if !self.match_token(TokenKind::RBrace) {
                    return Err(self.error(
                        "ERR_UNCLOSED_BRACE",
                        "Missing closing brace '}' in match statement",
                        "Right brace '}'",
                        "Statement -> Match Block",
                        "Add '}' to close match statement",
                    ));
                }

                if (has_ok_arm || has_err_arm) && (!has_ok_arm || !has_err_arm) && !has_wildcard {
                    return Err(self.error(
                        "ERR_NON_EXHAUSTIVE_MATCH",
                        "Result pattern match is not exhaustive: must handle both 'Ok($val)' and 'Err($code)', or provide wildcard '_' arm",
                        "Exhaustive Ok and Err arms or wildcard '_'",
                        "Statement -> Match Block",
                        "Add missing 'Ok(...)' / 'Err(...)' or wildcard '_ => { ... }' arm",
                    ));
                } else if !has_ok_arm && !has_err_arm && !has_wildcard {
                    return Err(self.error(
                        "ERR_NON_EXHAUSTIVE_MATCH",
                        "Value pattern match is not exhaustive: must provide a wildcard '_' fallback arm to ensure deterministic safety",
                        "Wildcard '_' arm",
                        "Statement -> Match Block",
                        "Add wildcard arm '_ => { ... }' at the end of match statement",
                    ));
                }

                for i in 0..arm_count {
                    self.patch_imm16(arm_jmp_ends[i], self.code_len as u16);
                }

                self.free_temp(match_reg);
                self.match_token(TokenKind::Semi);
            }

            TokenKind::Fn => {
                self.advance(); // consume 'fn'
                let name_tok = self.advance();
                if name_tok.kind != TokenKind::Ident {
                    return Err(self.error(
                        "ERR_EXPECTED_FN_NAME",
                        "Expected function identifier name after 'fn' keyword",
                        "Function name identifier",
                        "Statement -> Function Declaration",
                        "Specify function name e.g. 'fn my_func()'",
                    ));
                }
                let fn_name = &self.src[name_tok.start..name_tok.start + name_tok.len];
                if self.fn_count >= 16 {
                    return Err(self.error(
                        "ERR_MAX_FNS_EXCEEDED",
                        "Maximum static function limit reached (16 functions limit)",
                        "Fewer functions",
                        "Function Declaration",
                        "Reduce distinct function declarations in script",
                    ));
                }

                // Check redefinition
                for i in 0..self.fn_count {
                    let meta = &self.functions[i];
                    if meta.name_len == fn_name.len() && &meta.name[..meta.name_len] == fn_name {
                        return Err(self.error(
                            "ERR_FN_REDEFINED",
                            "Function is already defined",
                            "Unique function name",
                            "Function Declaration",
                            "Rename duplicate function",
                        ));
                    }
                }

                // Emit jump over function body so top-level control flow skips past it
                let jmp_skip = self.emit_imm16(PX64_OP_JMP, 0, 0)?;
                let entry_pc = self.code_len as u16;

                // Parameter list
                if !self.match_token(TokenKind::LParen) {
                    return Err(self.error(
                        "ERR_EXPECTED_LPAREN",
                        "Expected '(' after function name",
                        "Opening parenthesis '('",
                        "Statement -> Function Declaration",
                        "Add '(' after function name",
                    ));
                }

                let mut fn_meta = FnMeta::empty();
                fn_meta.name_len = core::cmp::min(fn_name.len(), 16);
                fn_meta.name[..fn_meta.name_len].copy_from_slice(&fn_name[..fn_meta.name_len]);
                fn_meta.entry_pc = entry_pc;

                while self.peek().kind != TokenKind::RParen && self.peek().kind != TokenKind::Eof {
                    let p_tok = self.advance();
                    if p_tok.kind != TokenKind::VarIdent {
                        return Err(self.error(
                            "ERR_EXPECTED_PARAM_VAR",
                            "Expected variable identifier '$param' in parameter list",
                            "Variable identifier '$param'",
                            "Statement -> Function Parameters",
                            "Use '$param' name in parameter list",
                        ));
                    }
                    let p_name = &self.src[p_tok.start..p_tok.start + p_tok.len];
                    let p_idx = fn_meta.param_count as usize;
                    if p_idx >= 4 {
                        return Err(self.error(
                            "ERR_MAX_PARAMS_EXCEEDED",
                            "Maximum parameter count exceeded (up to 4 parameters supported)",
                            "<= 4 parameters",
                            "Statement -> Function Parameters",
                            "Reduce parameter count to 4 or fewer",
                        ));
                    }
                    fn_meta.param_lens[p_idx] = core::cmp::min(p_name.len(), 16);
                    fn_meta.param_names[p_idx][..fn_meta.param_lens[p_idx]].copy_from_slice(&p_name[..fn_meta.param_lens[p_idx]]);
                    fn_meta.param_count += 1;
                    if !self.match_token(TokenKind::Comma) {
                        break;
                    }
                }

                if !self.match_token(TokenKind::RParen) {
                    return Err(self.error(
                        "ERR_EXPECTED_RPAREN",
                        "Expected ')' after parameter list",
                        "Closing parenthesis ')'",
                        "Statement -> Function Declaration",
                        "Add ')' to close parameter list",
                    ));
                }

                // Optional return type: '->' Ident
                if self.match_token(TokenKind::Arrow) {
                    self.advance(); // consume return type (e.g. i64)
                }

                let fn_idx = self.fn_count;
                self.functions[self.fn_count] = fn_meta;
                self.fn_count += 1;

                let prev_fn = self.current_fn;
                self.current_fn = Some(fn_idx);
                let saved_var_count = self.var_count;

                // Parse 0 or more @requires(condition) clauses
                while self.match_token(TokenKind::AtRequires) {
                    if !self.match_token(TokenKind::LParen) {
                        return Err(self.error(
                            "ERR_CONTRACT_SYNTAX",
                            "Expected '(' after @requires",
                            "Left parenthesis '('",
                            "Function Contract -> Precondition",
                            "Specify precondition as '@requires(condition)'",
                        ));
                    }
                    let cond_reg = self.alloc_temp()?;
                    self.expression(cond_reg)?;
                    if !self.match_token(TokenKind::RParen) {
                        return Err(self.error(
                            "ERR_UNCLOSED_PAREN",
                            "Missing closing parenthesis ')' in @requires condition",
                            "Closing parenthesis ')'",
                            "Function Contract -> Precondition",
                            "Add matching ')' after @requires condition",
                        ));
                    }
                    self.emit_inst(PX64_OP_ASSERT, cond_reg, 0, 0)?;
                    self.free_temp(cond_reg);
                }

                if !self.match_token(TokenKind::LBrace) {
                    return Err(self.error(
                        "ERR_EXPECTED_LBRACE",
                        "Expected opening brace '{' before function body",
                        "Left brace '{'",
                        "Statement -> Function Body",
                        "Add '{' to start function body",
                    ));
                }

                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    self.statement()?;
                }
                self.match_token(TokenKind::RBrace);

                // Default return at end of function
                self.emit_inst(PX64_OP_RET, 0, 0, 0)?;

                self.var_count = saved_var_count;
                self.current_fn = prev_fn;

                // Patch jump over function body
                self.patch_imm16(jmp_skip, self.code_len as u16);
            }

            TokenKind::Return => {
                self.advance(); // consume 'return'
                if self.peek().kind != TokenKind::Semi {
                    let ret_reg = 0; // $rax
                    self.expression(ret_reg)?;
                }
                self.match_token(TokenKind::Semi);
                self.emit_inst(PX64_OP_RET, 0, 0, 0)?;
            }

            TokenKind::Struct => {
                self.advance(); // consume 'struct'
                self.declare_struct_def()?;
            }

            TokenKind::Const => {
                self.advance(); // consume 'const'
                self.declare_const_table()?;
            }

            TokenKind::VarIdent | TokenKind::HardwareIdent => {
                if self.peek_ahead(1).kind == TokenKind::Dot {
                    let var_tok = self.advance();
                    self.match_token(TokenKind::Dot);
                    let field_tok = self.advance();
                    if field_tok.kind != TokenKind::Ident {
                        return Err(self.error(
                            "ERR_EXPECTED_FIELD_NAME",
                            "Expected field name identifier after '.'",
                            "Field identifier name",
                            "Statement -> Struct Field Assignment",
                            "Specify field name, e.g. '$pt.x := 10;'",
                        ));
                    }
                    let var_name = &self.src[var_tok.start..var_tok.start + var_tok.len];
                    let field_name = &self.src[field_tok.start..field_tok.start + field_tok.len];
                    let (inst_id, field_offset) = self.lookup_struct_field(var_name, field_name)?;

                    if !self.match_token(TokenKind::ColonEq) && !self.match_token(TokenKind::Eq) {
                        return Err(self.error(
                            "ERR_EXPECTED_COLON_EQ",
                            "Expected ':=' or '=' in struct field assignment",
                            "':=' or '='",
                            "Statement -> Struct Field Assignment",
                            "Assign field value with '$inst.field := val;'",
                        ));
                    }

                    let val_reg = self.alloc_temp()?;
                    self.expression(val_reg)?;
                    self.match_token(TokenKind::Semi);
                    self.emit_inst(PX64_OP_STRUCT_STORE, inst_id, field_offset, val_reg)?;
                    self.free_temp(val_reg);
                    return Ok(());
                }

                if self.peek_ahead(1).kind == TokenKind::LBracket {
                    let ident = self.advance();
                    let arr_id = self.lookup_array(ident)?;
                    self.match_token(TokenKind::LBracket);
                    let idx_reg = self.alloc_temp()?;
                    self.expression(idx_reg)?;
                    self.match_token(TokenKind::RBracket);
                    if !self.match_token(TokenKind::ColonEq) && !self.match_token(TokenKind::Eq) {
                        return Err(self.error(
                            "ERR_EXPECTED_COLON_EQ",
                            "Expected ':=' in array element assignment",
                            "':='",
                            "Statement -> Array Assignment",
                            "Assign value with '$arr[$idx] := val;'",
                        ));
                    }
                    let val_reg = self.alloc_temp()?;
                    self.expression(val_reg)?;
                    self.match_token(TokenKind::Semi);
                    self.emit_inst(PX64_OP_ARR_STORE, arr_id, idx_reg, val_reg)?;
                    self.free_temp(val_reg);
                    self.free_temp(idx_reg);
                    return Ok(());
                }

                let ident = self.advance();
                let var_reg = self.resolve_var(ident)?;

                if self.match_token(TokenKind::ColonEq) || self.match_token(TokenKind::Eq) {
                    self.check_var_mutation(ident)?;
                    // Handle hardware handle allocation tracking
                    if (16..=19).contains(&var_reg) {
                        let slot = (var_reg - 16) as usize;
                        if let HandleState::Allocated { .. } = self.handle_states[slot] {
                            return Err(self.error(
                                "ERR_LINEAR_OVERWRITE",
                                "Hardware handle overwritten before previous buffer was consumed/sent",
                                "Consume prior handle with @send(#h) before reassigning",
                                "Linear Ownership Verification",
                                "Ensure '@send(#h)' is invoked before overwriting '#h := @capture()'",
                            ));
                        }
                        self.handle_states[slot] = HandleState::Allocated {
                            line: ident.line,
                            col: ident.col,
                        };
                    }
                    self.expression(var_reg)?;
                    if !self.match_token(TokenKind::Semi) {
                        return Err(self.error(
                            "ERR_MISSING_SEMICOLON",
                            "Missing semicolon ';' at end of assignment",
                            "Semicolon ';'",
                            "Statement -> Variable Assignment",
                            "Append ';' at end of assignment statement",
                        ));
                    }
                    return Ok(());
                } else if self.match_token(TokenKind::PlusEq) {
                    self.check_var_mutation(ident)?;
                    if let TokenKind::Number(n) = self.peek().kind {
                        if (0..=255).contains(&n) {
                            self.advance();
                            self.emit_inst(PX64_OP_ADDI, var_reg, var_reg, n as u8)?;
                            if !self.match_token(TokenKind::Semi) {
                                return Err(self.error(
                                    "ERR_MISSING_SEMICOLON",
                                    "Missing semicolon ';' at end of compound assignment",
                                    "Semicolon ';'",
                                    "Statement -> Compound Addition Assignment",
                                    "Append ';' at end of statement",
                                ));
                            }
                            return Ok(());
                        }
                    }
                    let tmp_reg = self.alloc_temp()?;
                    self.expression(tmp_reg)?;
                    self.emit_inst(PX64_OP_ADD, var_reg, var_reg, tmp_reg)?;
                    self.free_temp(tmp_reg);
                    if !self.match_token(TokenKind::Semi) {
                        return Err(self.error(
                            "ERR_MISSING_SEMICOLON",
                            "Missing semicolon ';' at end of compound assignment",
                            "Semicolon ';'",
                            "Statement -> Compound Addition Assignment",
                            "Append ';' at end of statement",
                        ));
                    }
                    return Ok(());
                } else if self.match_token(TokenKind::MinusEq) {
                    self.check_var_mutation(ident)?;
                    if let TokenKind::Number(n) = self.peek().kind {
                        if (0..=255).contains(&n) {
                            self.advance();
                            self.emit_inst(PX64_OP_SUBI, var_reg, var_reg, n as u8)?;
                            if !self.match_token(TokenKind::Semi) {
                                return Err(self.error(
                                    "ERR_MISSING_SEMICOLON",
                                    "Missing semicolon ';' at end of compound assignment",
                                    "Semicolon ';'",
                                    "Statement -> Compound Subtraction Assignment",
                                    "Append ';' at end of statement",
                                ));
                            }
                            return Ok(());
                        }
                    }
                    let tmp_reg = self.alloc_temp()?;
                    self.expression(tmp_reg)?;
                    self.emit_inst(PX64_OP_SUB, var_reg, var_reg, tmp_reg)?;
                    self.free_temp(tmp_reg);
                    if !self.match_token(TokenKind::Semi) {
                        return Err(self.error(
                            "ERR_MISSING_SEMICOLON",
                            "Missing semicolon ';' at end of compound assignment",
                            "Semicolon ';'",
                            "Statement -> Compound Subtraction Assignment",
                            "Append ';' at end of statement",
                        ));
                    }
                    return Ok(());
                } else {
                    self.current -= 1;
                    self.expression_statement()?;
                    return Ok(());
                }
            }

            _ => {
                self.expression_statement()?;
            }
        }
        Ok(())
    }

    fn expression_statement(&mut self) -> Result<(), CompileError> {
        self.expression(0)?;
        if !self.match_token(TokenKind::Semi) {
            return Err(self.error(
                "ERR_MISSING_SEMICOLON",
                "Missing semicolon ';' at end of expression statement",
                "Semicolon ';'",
                "Statement -> Expression Statement",
                "Append ';' to terminate the statement",
            ));
        }
        Ok(())
    }

    fn expression(&mut self, dst: u8) -> Result<(), CompileError> {
        let val = self.ternary(dst)?;
        if let Some(v) = val {
            self.emit_const(dst, v)?;
        }
        while self.match_token(TokenKind::Pipe) {
            if self.peek().kind == TokenKind::IntrinsicIdent || self.peek().kind == TokenKind::Ident {
                let tok = self.advance();
                let name = &self.src[tok.start..tok.start + tok.len];
                let func_id = match name {
                    b"@send" | b"net.send" => {
                        if (16..=19).contains(&dst) {
                            let slot = (dst - 16) as usize;
                            match self.handle_states[slot] {
                                HandleState::Unallocated => {
                                    return Err(self.error(
                                        "ERR_LINEAR_USE_BEFORE_ALLOC",
                                        "Hardware handle sent before being captured/allocated",
                                        "Capture handle with '#f := @capture()' before sending",
                                        "Linear Ownership Verification",
                                        "Initialize hardware handle with '@capture()' before '@send()'",
                                    ));
                                }
                                HandleState::Consumed => {
                                    return Err(self.error(
                                        "ERR_LINEAR_DOUBLE_SEND",
                                        "Hardware handle sent multiple times (double-send / double-free violation)",
                                        "Single @send per allocated handle",
                                        "Linear Ownership Verification",
                                        "Remove duplicate '@send(#f);' calls on the same handle",
                                    ));
                                }
                                HandleState::Allocated { .. } => {
                                    self.handle_states[slot] = HandleState::Consumed;
                                }
                            }
                        }
                        NATIVE_NET_SEND
                    }
                    b"@print" | b"print" => NATIVE_PRINT,
                    b"@println" | b"println" => NATIVE_PRINTLN,
                    b"@rate" | b"net.set_rate" => NATIVE_NET_SET_RATE,
                    _ => {
                        return Err(self.error(
                            "ERR_INVALID_PIPE_TARGET",
                            "Invalid pipe target function",
                            "One of @send, @print, @println, @rate",
                            "Expression -> Pipe",
                            "Pipe value to a valid intrinsic like |> @send",
                        ));
                    }
                };
                self.emit_inst(PX64_OP_CALL_NAT, dst, func_id, dst)?;
            }
        }
        Ok(())
    }

    fn ternary(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let val = self.logic_or(dst)?;
        if self.match_token(TokenKind::Question) {
            if let Some(v) = val {
                self.emit_const(dst, v)?;
            }
            let jz_pos = self.emit_inst(PX64_OP_JZ, dst, 0, 0)?;

            // True branch
            if self.match_token(TokenKind::LBrace) {
                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    self.statement()?;
                }
                self.match_token(TokenKind::RBrace);
            } else {
                self.expression(dst)?;
            }

            if self.match_token(TokenKind::Colon) {
                let jmp_end_pos = self.emit_inst(PX64_OP_JMP, 0, 0, 0)?;
                self.patch_imm16(jz_pos, self.code_len as u16);

                // False branch
                if self.match_token(TokenKind::LBrace) {
                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        self.statement()?;
                    }
                    self.match_token(TokenKind::RBrace);
                } else {
                    self.expression(dst)?;
                }
                self.patch_imm16(jmp_end_pos, self.code_len as u16);
            } else {
                self.patch_imm16(jz_pos, self.code_len as u16);
            }
            return Ok(None);
        }
        Ok(val)
    }

    fn logic_or(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.logic_and(dst)?;
        while self.peek().kind == TokenKind::OrOp {
            self.advance();
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.logic_and(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(if a != 0 || b != 0 { 1 } else { 0 });
            } else {
                if let Some(a) = val {
                    self.emit_const(dst, a)?;
                    val = None;
                }
                if let Some(b) = rhs_val {
                    self.emit_const(rhs_reg, b)?;
                }
                let zero_reg = self.alloc_temp()?;
                self.emit_const(zero_reg, 0)?;
                self.emit_inst(PX64_OP_CMP_NE, dst, dst, zero_reg)?;
                let rhs_bool = self.alloc_temp()?;
                self.emit_inst(PX64_OP_CMP_NE, rhs_bool, rhs_reg, zero_reg)?;
                self.emit_inst(PX64_OP_OR, dst, dst, rhs_bool)?;
                self.free_temp(rhs_bool);
                self.free_temp(zero_reg);
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn logic_and(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.equality(dst)?;
        while self.peek().kind == TokenKind::And {
            self.advance();
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.equality(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(if a != 0 && b != 0 { 1 } else { 0 });
            } else {
                if let Some(a) = val {
                    self.emit_const(dst, a)?;
                    val = None;
                }
                if let Some(b) = rhs_val {
                    self.emit_const(rhs_reg, b)?;
                }
                let zero_reg = self.alloc_temp()?;
                self.emit_const(zero_reg, 0)?;
                self.emit_inst(PX64_OP_CMP_NE, dst, dst, zero_reg)?;
                let rhs_bool = self.alloc_temp()?;
                self.emit_inst(PX64_OP_CMP_NE, rhs_bool, rhs_reg, zero_reg)?;
                self.emit_inst(PX64_OP_AND, dst, dst, rhs_bool)?;
                self.free_temp(rhs_bool);
                self.free_temp(zero_reg);
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn equality(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.comparison(dst)?;
        while self.peek().kind == TokenKind::EqEq || self.peek().kind == TokenKind::NotEq {
            let op = self.advance().kind;
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.comparison(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(if op == TokenKind::EqEq {
                    if a == b { 1 } else { 0 }
                } else if a != b {
                    1
                } else {
                    0
                });
            } else {
                if let Some(a) = val {
                    self.emit_const(dst, a)?;
                    val = None;
                }
                if let Some(b) = rhs_val {
                    self.emit_const(rhs_reg, b)?;
                }
                if op == TokenKind::EqEq {
                    self.emit_inst(PX64_OP_CMP_EQ, dst, dst, rhs_reg)?;
                } else {
                    self.emit_inst(PX64_OP_CMP_NE, dst, dst, rhs_reg)?;
                }
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn comparison(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.bitwise_or(dst)?;
        while matches!(
            self.peek().kind,
            TokenKind::Lt | TokenKind::LtEq | TokenKind::Gt | TokenKind::GtEq
        ) {
            let op = self.advance().kind;
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.bitwise_or(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(match op {
                    TokenKind::Lt => if a < b { 1 } else { 0 },
                    TokenKind::LtEq => if a <= b { 1 } else { 0 },
                    TokenKind::Gt => if a > b { 1 } else { 0 },
                    TokenKind::GtEq => if a >= b { 1 } else { 0 },
                    _ => 0,
                });
            } else {
                if let Some(a) = val {
                    self.emit_const(dst, a)?;
                    val = None;
                }
                if let Some(b) = rhs_val {
                    self.emit_const(rhs_reg, b)?;
                }
                match op {
                    TokenKind::Lt => {
                        self.emit_inst(PX64_OP_CMP_LT, dst, dst, rhs_reg)?;
                    }
                    TokenKind::LtEq => {
                        self.emit_inst(PX64_OP_CMP_LE, dst, dst, rhs_reg)?;
                    }
                    TokenKind::Gt => {
                        self.emit_inst(PX64_OP_CMP_GT, dst, dst, rhs_reg)?;
                    }
                    TokenKind::GtEq => {
                        self.emit_inst(PX64_OP_CMP_GE, dst, dst, rhs_reg)?;
                    }
                    _ => {}
                }
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn bitwise_or(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.bitwise_xor(dst)?;
        while self.peek().kind == TokenKind::PipeSingle {
            self.advance();
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.bitwise_xor(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(a | b);
            } else {
                if let Some(a) = val {
                    self.emit_const(dst, a)?;
                    val = None;
                }
                if let Some(b) = rhs_val {
                    self.emit_const(rhs_reg, b)?;
                }
                self.emit_inst(PX64_OP_OR, dst, dst, rhs_reg)?;
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn bitwise_xor(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.bitwise_and(dst)?;
        while self.peek().kind == TokenKind::Caret {
            self.advance();
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.bitwise_and(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(a ^ b);
            } else {
                if let Some(a) = val {
                    self.emit_const(dst, a)?;
                    val = None;
                }
                if let Some(b) = rhs_val {
                    self.emit_const(rhs_reg, b)?;
                }
                self.emit_inst(PX64_OP_XOR, dst, dst, rhs_reg)?;
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn bitwise_and(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.shift(dst)?;
        while self.peek().kind == TokenKind::Amp {
            self.advance();
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.shift(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(a & b);
            } else {
                if let Some(a) = val {
                    self.emit_const(dst, a)?;
                    val = None;
                }
                if let Some(b) = rhs_val {
                    self.emit_const(rhs_reg, b)?;
                }
                self.emit_inst(PX64_OP_AND, dst, dst, rhs_reg)?;
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn shift(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.term(dst)?;
        while matches!(self.peek().kind, TokenKind::Shl | TokenKind::Shr) {
            let op = self.advance().kind;
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.term(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(match op {
                    TokenKind::Shl => a.wrapping_shl((b & 63) as u32),
                    TokenKind::Shr => (a as u64 >> (b & 63)) as i64,
                    _ => 0,
                });
            } else {
                if let Some(a) = val {
                    self.emit_const(dst, a)?;
                    val = None;
                }
                if let Some(b) = rhs_val {
                    self.emit_const(rhs_reg, b)?;
                }
                match op {
                    TokenKind::Shl => {
                        self.emit_inst(PX64_OP_SHL, dst, dst, rhs_reg)?;
                    }
                    TokenKind::Shr => {
                        self.emit_inst(PX64_OP_SHR, dst, dst, rhs_reg)?;
                    }
                    _ => {}
                }
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn term(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.factor(dst)?;
        while self.peek().kind == TokenKind::Plus || self.peek().kind == TokenKind::Minus {
            let op = self.advance().kind;
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.factor(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(match op {
                    TokenKind::Plus => a.wrapping_add(b),
                    TokenKind::Minus => a.wrapping_sub(b),
                    _ => 0,
                });
            } else {
                if let Some(a) = val {
                    self.emit_const(dst, a)?;
                    val = None;
                }
                if let Some(b) = rhs_val {
                    self.emit_const(rhs_reg, b)?;
                }
                if op == TokenKind::Plus {
                    self.emit_inst(PX64_OP_ADD, dst, dst, rhs_reg)?;
                } else {
                    self.emit_inst(PX64_OP_SUB, dst, dst, rhs_reg)?;
                }
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn factor(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.primary(dst)?;
        while self.peek().kind == TokenKind::Star
            || self.peek().kind == TokenKind::Slash
            || self.peek().kind == TokenKind::Percent
        {
            let op = self.advance().kind;
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.primary(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(match op {
                    TokenKind::Star => a.wrapping_mul(b),
                    TokenKind::Slash => {
                        if b != 0 {
                            a.wrapping_div(b)
                        } else {
                            0
                        }
                    }
                    TokenKind::Percent => {
                        if b != 0 {
                            a.wrapping_rem(b)
                        } else {
                            0
                        }
                    }
                    _ => 0,
                });
            } else {
                if let Some(a) = val {
                    self.emit_const(dst, a)?;
                    val = None;
                }
                if let Some(b) = rhs_val {
                    self.emit_const(rhs_reg, b)?;
                }
                match op {
                    TokenKind::Star => {
                        self.emit_inst(PX64_OP_MUL, dst, dst, rhs_reg)?;
                    }
                    TokenKind::Slash => {
                        self.emit_inst(PX64_OP_DIV, dst, dst, rhs_reg)?;
                    }
                    TokenKind::Percent => {
                        self.emit_inst(PX64_OP_MOD, dst, dst, rhs_reg)?;
                    }
                    _ => {}
                }
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn primary(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let tok = self.advance();

        match tok.kind {
            TokenKind::Number(n) => Ok(Some(n)),

            TokenKind::TimeLiteral(ns) => Ok(Some(ns as i64)),

            TokenKind::StringLit => {
                let s = if tok.len >= 2 {
                    &self.src[tok.start + 1..tok.start + tok.len - 1]
                } else {
                    &[]
                };
                if self.str_pool_len + s.len() > MAX_STRING_POOL {
                    return Err(self.error(
                        "ERR_STRING_POOL_FULL",
                        "String literal pool exhausted (512 bytes limit reached)",
                        "Shorter string constants",
                        "String Pool Allocation",
                        "Reduce the size of string constants",
                    ));
                }
                let offset = self.str_pool_len;
                self.str_pool[offset..offset + s.len()].copy_from_slice(s);
                self.str_pool_len += s.len();

                self.emit_inst(PX64_OP_MOV_STR, dst, offset as u8, s.len() as u8)?;
                Ok(None)
            }

            TokenKind::VarIdent | TokenKind::HardwareIdent => {
                if self.peek().kind == TokenKind::LBracket {
                    let arr_id = self.lookup_array(tok)?;
                    self.advance(); // consume '['
                    let idx_reg = self.alloc_temp()?;
                    self.expression(idx_reg)?;
                    self.match_token(TokenKind::RBracket);
                    self.emit_inst(PX64_OP_ARR_LOAD, dst, arr_id, idx_reg)?;
                    self.free_temp(idx_reg);
                    Ok(None)
                } else if self.peek().kind == TokenKind::Dot {
                    self.advance(); // consume '.'
                    let field_tok = self.advance();
                    if field_tok.kind != TokenKind::Ident {
                        return Err(self.error(
                            "ERR_EXPECTED_FIELD_NAME",
                            "Expected field name identifier after '.'",
                            "Field identifier name",
                            "Expression -> Struct Field Access",
                            "Specify field name, e.g. '$pt.x'",
                        ));
                    }
                    let var_name = &self.src[tok.start..tok.start + tok.len];
                    let field_name = &self.src[field_tok.start..field_tok.start + field_tok.len];
                    let (inst_id, field_offset) = self.lookup_struct_field(var_name, field_name)?;
                    self.emit_inst(PX64_OP_STRUCT_LOAD, dst, inst_id, field_offset)?;
                    Ok(None)
                } else {
                    let var_reg = self.resolve_var(tok)?;
                    if var_reg != dst {
                        self.emit_inst(PX64_OP_MOV_REG, dst, var_reg, 0)?;
                    }
                    Ok(None)
                }
            }

            TokenKind::IntrinsicIdent | TokenKind::Ident => {
                let name = &self.src[tok.start..tok.start + tok.len];
                if self.peek().kind == TokenKind::LBracket {
                    let (tbl_id, _len) = self.lookup_const_table(name)?;
                    self.advance(); // consume '['
                    let idx_reg = self.alloc_temp()?;
                    self.expression(idx_reg)?;
                    self.match_token(TokenKind::RBracket);
                    self.emit_inst(PX64_OP_TBL_LOAD, dst, tbl_id, idx_reg)?;
                    self.free_temp(idx_reg);
                    return Ok(None);
                }

                if self.peek().kind == TokenKind::LParen {
                    // Check if calling a user-defined static function
                    let mut fn_match = None;
                    for i in 0..self.fn_count {
                        let meta = &self.functions[i];
                        if meta.name_len == name.len() && &meta.name[..meta.name_len] == name {
                            fn_match = Some(*meta);
                            break;
                        }
                    }

                    if let Some(fn_meta) = fn_match {
                        self.advance(); // consume '('
                        let param_regs: [u8; 4] = [7, 6, 2, 1]; // $rdi, $rsi, $rdx, $rcx
                        let mut arg_idx = 0;
                        while self.peek().kind != TokenKind::RParen && self.peek().kind != TokenKind::Eof {
                            if arg_idx >= fn_meta.param_count as usize {
                                return Err(self.error(
                                    "ERR_TOO_MANY_ARGS",
                                    "Too many arguments passed to static function",
                                    "Matching number of function parameters",
                                    "Expression -> Function Call",
                                    "Check function parameter signature",
                                ));
                            }
                            let target_reg = param_regs[arg_idx];
                            self.expression(target_reg)?;
                            arg_idx += 1;
                            if !self.match_token(TokenKind::Comma) {
                                break;
                            }
                        }
                        if arg_idx < fn_meta.param_count as usize {
                            return Err(self.error(
                                "ERR_TOO_FEW_ARGS",
                                "Too few arguments passed to static function",
                                "Matching number of function parameters",
                                "Expression -> Function Call",
                                "Provide all required function arguments",
                            ));
                        }
                        if !self.match_token(TokenKind::RParen) {
                            return Err(self.error(
                                "ERR_UNCLOSED_PAREN",
                                "Missing closing parenthesis ')' in function argument list",
                                "Closing parenthesis ')'",
                                "Expression -> Function Call",
                                "Add matching ')' after argument list",
                            ));
                        }
                        self.emit_imm16(PX64_OP_CALL, 0, fn_meta.entry_pc)?;
                        if dst != 0 {
                            self.emit_inst(PX64_OP_MOV_REG, dst, 0, 0)?;
                        }
                        return Ok(None);
                    }

                    if name == b"@streq" || name == b"str.eq" {
                        self.advance(); // consume '('
                        let s1_reg = self.alloc_temp()?;
                        self.expression(s1_reg)?;
                        if !self.match_token(TokenKind::Comma) {
                            return Err(self.error(
                                "ERR_EXPECTED_COMMA",
                                "Expected ',' between arguments in @streq($s1, $s2)",
                                "Comma ','",
                                "Expression -> Intrinsic Function Call",
                                "Provide two string arguments, e.g. '@streq($s1, $s2)'",
                            ));
                        }
                        let s2_reg = self.alloc_temp()?;
                        self.expression(s2_reg)?;
                        if !self.match_token(TokenKind::RParen) {
                            return Err(self.error(
                                "ERR_UNCLOSED_PAREN",
                                "Missing closing parenthesis ')' in @streq($s1, $s2)",
                                "Closing parenthesis ')'",
                                "Expression -> Intrinsic Function Call",
                                "Add matching ')' after argument list",
                            ));
                        }
                        self.emit_inst(PX64_OP_STREQ, dst, s1_reg, s2_reg)?;
                        self.free_temp(s2_reg);
                        self.free_temp(s1_reg);
                        return Ok(None);
                    }

                    if name == b"@zip_with" {
                        self.advance(); // consume '('
                        let v1 = self.parse_view_source()?;
                        if !self.match_token(TokenKind::Comma) {
                            return Err(self.error(
                                "ERR_EXPECTED_COMMA",
                                "Expected ',' between views in @zip_with",
                                "Comma ','",
                                "Expression -> Combinator Call",
                                "Provide two views and function, e.g. '@zip_with($v1, $v2, mul)'",
                            ));
                        }
                        let v2 = self.parse_view_source()?;
                        if !self.match_token(TokenKind::Comma) {
                            return Err(self.error(
                                "ERR_EXPECTED_COMMA",
                                "Expected ',' before function name in @zip_with",
                                "Comma ','",
                                "Expression -> Combinator Call",
                                "Provide function name, e.g. '@zip_with($v1, $v2, mul)'",
                            ));
                        }
                        let fn_tok = self.advance();
                        let fn_name = &self.src[fn_tok.start..fn_tok.start + fn_tok.len];
                        if !self.match_token(TokenKind::RParen) {
                            return Err(self.error(
                                "ERR_UNCLOSED_PAREN",
                                "Missing closing parenthesis ')' in @zip_with",
                                "Closing parenthesis ')'",
                                "Expression -> Combinator Call",
                                "Close with ')'",
                            ));
                        }

                        // Check if chained with |> @sum()
                        let mut is_sum = false;
                        if self.peek().kind == TokenKind::Pipe {
                            let next_tok = self.peek_ahead(1);
                            let next_name = &self.src[next_tok.start..next_tok.start + next_tok.len];
                            if next_name == b"@sum" || next_name == b"sum" {
                                self.advance(); // consume '|>'
                                self.advance(); // consume '@sum'
                                if self.peek().kind == TokenKind::LParen {
                                    self.advance();
                                    self.match_token(TokenKind::RParen);
                                }
                                is_sum = true;
                            }
                        }

                        self.compile_zip_with_to_reg(dst, v1, v2, fn_name, is_sum)?;
                        return Ok(None);
                    }

                    if name == b"@sum" || name == b"sum" {
                        self.advance(); // consume '('
                        if self.peek().kind == TokenKind::RParen {
                            self.advance();
                            return Ok(None);
                        }
                        let v = self.parse_view_source()?;
                        if !self.match_token(TokenKind::RParen) {
                            return Err(self.error(
                                "ERR_UNCLOSED_PAREN",
                                "Missing closing parenthesis ')' in @sum",
                                "Closing parenthesis ')'",
                                "Expression -> Combinator Call",
                                "Close with ')'",
                            ));
                        }
                        self.compile_sum_to_reg(dst, v)?;
                        return Ok(None);
                    }

                    if name == b"@reduce" || name == b"reduce" {
                        self.advance(); // consume '('
                        let v = self.parse_view_source()?;
                        if !self.match_token(TokenKind::Comma) {
                            return Err(self.error(
                                "ERR_EXPECTED_COMMA",
                                "Expected ',' after view in @reduce",
                                "Comma ','",
                                "Expression -> Combinator Call",
                                "Provide view, initial value, and reducer function",
                            ));
                        }
                        let init_reg = self.alloc_temp()?;
                        self.expression(init_reg)?;
                        if !self.match_token(TokenKind::Comma) {
                            return Err(self.error(
                                "ERR_EXPECTED_COMMA",
                                "Expected ',' before function name in @reduce",
                                "Comma ','",
                                "Expression -> Combinator Call",
                                "Provide reducer function name",
                            ));
                        }
                        let fn_tok = self.advance();
                        let fn_name = &self.src[fn_tok.start..fn_tok.start + fn_tok.len];
                        if !self.match_token(TokenKind::RParen) {
                            return Err(self.error(
                                "ERR_UNCLOSED_PAREN",
                                "Missing closing parenthesis ')' in @reduce",
                                "Closing parenthesis ')'",
                                "Expression -> Combinator Call",
                                "Close with ')'",
                            ));
                        }
                        self.emit_inst(PX64_OP_MOV_REG, dst, init_reg, 0)?;
                        self.free_temp(init_reg);
                        self.compile_reduce_to_reg(dst, v, None, fn_name)?;
                        return Ok(None);
                    }

                    let (func_id, arity) = match name {
                        b"@print" | b"print" => (NATIVE_PRINT, 1),
                        b"@println" | b"println" => (NATIVE_PRINTLN, 1),
                        b"@tsc" | b"sys.tsc" => (NATIVE_SYS_TSC, 0),
                        b"@rtt" | b"net.rtt" => (NATIVE_NET_RTT, 0),
                        b"@rate" | b"net.set_rate" => (NATIVE_NET_SET_RATE, 1),
                        b"@capture" | b"gpu.capture" => (NATIVE_GPU_CAPTURE, 0),
                        b"@send" | b"net.send" => (NATIVE_NET_SEND, 1),
                        b"@argc" | b"sys.argc" => (NATIVE_SCRIPT_ARGC, 0),
                        b"@arg" | b"sys.arg" => (NATIVE_SCRIPT_ARG, 1),
                        b"@ok" => (NATIVE_TAG_OK, 1),
                        b"@err" => (NATIVE_TAG_ERR, 1),
                        b"@is_ok" => (NATIVE_IS_OK, 1),
                        b"@is_err" => (NATIVE_IS_ERR, 1),
                        b"@unwrap" => (NATIVE_UNWRAP, 1),
                        b"@core_id" | b"sys.core_id" => (NATIVE_CORE_ID, 0),
                        b"@tsc_freq" | b"sys.tsc_freq" => (NATIVE_TSC_FREQ, 0),
                        b"@uptime_ns" | b"sys.uptime_ns" => (NATIVE_UPTIME_NS, 0),
                        b"@busy_wait" | b"sys.busy_wait" => (NATIVE_BUSY_WAIT, 1),
                        b"@ring_depth" | b"sys.ring_depth" => (NATIVE_RING_DEPTH, 1),
                        b"@min" | b"math.min" => (NATIVE_MATH_MIN, 2),
                        b"@max" | b"math.max" => (NATIVE_MATH_MAX, 2),
                        b"@abs" | b"math.abs" => (NATIVE_MATH_ABS, 1),
                        b"@clamp" | b"math.clamp" => (NATIVE_MATH_CLAMP, 3),
                        b"@popcnt" | b"bit.popcnt" => (NATIVE_BIT_POPCNT, 1),
                        b"@lzcnt" | b"bit.lzcnt" => (NATIVE_BIT_LZCNT, 1),
                        b"@crc32" | b"hash.crc32" => (NATIVE_CRC32, 2),
                        b"@vram_read" | b"vram.read" => (NATIVE_VRAM_READ, 2),
                        b"@vram_write" | b"vram.write" => (NATIVE_VRAM_WRITE, 3),
                        _ => {
                            return Err(self.error(
                                "ERR_UNKNOWN_INTRINSIC",
                                "Unknown intrinsic function name",
                                "Supported intrinsics: @print, @println, @tsc, @rtt, @rate, @capture, @send, @argc, @arg, @ok, @err, @is_ok, @is_err, @unwrap, @core_id, @tsc_freq, @uptime_ns, @busy_wait, @ring_depth, @min, @max, @abs, @clamp, @popcnt, @lzcnt, @crc32, @vram_read, @vram_write",
                                "Expression -> Intrinsic Call",
                                "Verify that the intrinsic name matches supported DSL intrinsics",
                            ));
                        }
                    };

                    self.advance(); // consume '('
                    let mut arg0_reg = 0u8;
                    let mut arg1_reg = 0u8;
                    let mut arg2_reg = 0u8;
                    let mut raw_h_reg = 0u8;

                    if arity == 0 {
                        // 0 arguments: allow empty ()
                    } else if arity == 1 {
                        if self.peek().kind != TokenKind::RParen {
                            let arg_tok = self.peek();
                            if arg_tok.kind == TokenKind::HardwareIdent {
                                if let Ok(reg) = self.resolve_var(arg_tok) {
                                    raw_h_reg = reg;
                                }
                            }
                            arg0_reg = self.alloc_temp()?;
                            self.expression(arg0_reg)?;
                        }
                    } else if arity == 2 {
                        arg0_reg = self.alloc_temp()?;
                        self.expression(arg0_reg)?;
                        if !self.match_token(TokenKind::Comma) {
                            return Err(self.error(
                                "ERR_EXPECTED_COMMA",
                                "Expected ',' between 2 intrinsic arguments",
                                "Comma ','",
                                "Expression -> Intrinsic Function Call",
                                "Provide 2 arguments separated by a comma",
                            ));
                        }
                        arg1_reg = self.alloc_temp()?;
                        self.expression(arg1_reg)?;
                    } else if arity == 3 {
                        arg0_reg = self.alloc_temp()?;
                        self.expression(arg0_reg)?;
                        if !self.match_token(TokenKind::Comma) {
                            return Err(self.error(
                                "ERR_EXPECTED_COMMA",
                                "Expected ',' between intrinsic arguments",
                                "Comma ','",
                                "Expression -> Intrinsic Function Call",
                                "Provide 3 arguments separated by commas",
                            ));
                        }
                        arg1_reg = self.alloc_temp()?;
                        self.expression(arg1_reg)?;
                        if !self.match_token(TokenKind::Comma) {
                            return Err(self.error(
                                "ERR_EXPECTED_COMMA",
                                "Expected ',' between intrinsic arguments",
                                "Comma ','",
                                "Expression -> Intrinsic Function Call",
                                "Provide 3 arguments separated by commas",
                            ));
                        }
                        arg2_reg = self.alloc_temp()?;
                        self.expression(arg2_reg)?;
                    }

                    if !self.match_token(TokenKind::RParen) {
                        return Err(self.error(
                            "ERR_UNCLOSED_PAREN",
                            "Missing closing parenthesis ')' in function argument list",
                            "Closing parenthesis ')'",
                            "Expression -> Intrinsic Function Call",
                            "Add matching ')' after argument list",
                        ));
                    }

                    if func_id == NATIVE_NET_SEND {
                        let target_reg = if (16..=19).contains(&raw_h_reg) {
                            raw_h_reg
                        } else {
                            arg0_reg
                        };
                        if (16..=19).contains(&target_reg) {
                            let slot = (target_reg - 16) as usize;
                            match self.handle_states[slot] {
                                HandleState::Unallocated => {
                                    return Err(self.error(
                                        "ERR_LINEAR_USE_BEFORE_ALLOC",
                                        "Hardware handle sent before being captured/allocated",
                                        "Capture handle with '#f := @capture()' before sending",
                                        "Linear Ownership Verification",
                                        "Initialize hardware handle with '@capture()' before '@send()'",
                                    ));
                                }
                                HandleState::Consumed => {
                                    return Err(self.error(
                                        "ERR_LINEAR_DOUBLE_SEND",
                                        "Hardware handle sent multiple times (double-send / double-free violation)",
                                        "Single @send per allocated handle",
                                        "Linear Ownership Verification",
                                        "Remove duplicate '@send(#f);' calls on the same handle",
                                    ));
                                }
                                HandleState::Allocated { .. } => {
                                    self.handle_states[slot] = HandleState::Consumed;
                                }
                            }
                        }
                    }

                    if arity == 0 {
                        self.emit_inst(PX64_OP_CALL_NAT, dst, func_id, 0)?;
                    } else if arity == 1 {
                        self.emit_inst(PX64_OP_CALL_NAT, dst, func_id, arg0_reg)?;
                        if arg0_reg != 0 {
                            self.free_temp(arg0_reg);
                        }
                    } else if arity == 2 {
                        self.emit_inst(PX64_OP_CALL_NAT, dst, func_id, arg0_reg)?;
                        self.free_temp(arg1_reg);
                        self.free_temp(arg0_reg);
                    } else if arity == 3 {
                        self.emit_inst(PX64_OP_CALL_NAT, dst, func_id, arg0_reg)?;
                        self.free_temp(arg2_reg);
                        self.free_temp(arg1_reg);
                        self.free_temp(arg0_reg);
                    }
                    Ok(None)
                } else {
                    let var_reg = self.resolve_var(tok)?;
                    if var_reg != dst {
                        self.emit_inst(PX64_OP_MOV_REG, dst, var_reg, 0)?;
                    }
                    Ok(None)
                }
            }

            TokenKind::Minus => {
                let zero_reg = self.alloc_temp()?;
                self.emit_const(zero_reg, 0)?;
                let rhs_reg = self.alloc_temp()?;
                let rhs_val = self.primary(rhs_reg)?;
                if let Some(b) = rhs_val {
                    self.free_temp(rhs_reg);
                    self.free_temp(zero_reg);
                    Ok(Some(-b))
                } else {
                    self.emit_inst(PX64_OP_SUB, dst, zero_reg, rhs_reg)?;
                    self.free_temp(rhs_reg);
                    self.free_temp(zero_reg);
                    Ok(None)
                }
            }

            TokenKind::Exclamation => {
                let rhs_reg = self.alloc_temp()?;
                let rhs_val = self.primary(rhs_reg)?;
                if let Some(b) = rhs_val {
                    self.free_temp(rhs_reg);
                    Ok(Some(if b == 0 { 1 } else { 0 }))
                } else {
                    let zero_reg = self.alloc_temp()?;
                    self.emit_const(zero_reg, 0)?;
                    self.emit_inst(PX64_OP_CMP_EQ, dst, rhs_reg, zero_reg)?;
                    self.free_temp(zero_reg);
                    self.free_temp(rhs_reg);
                    Ok(None)
                }
            }

            TokenKind::LParen => {
                let val = self.ternary(dst)?;
                if !self.match_token(TokenKind::RParen) {
                    return Err(self.error(
                        "ERR_UNCLOSED_PAREN",
                        "Expected closing parenthesis ')' after sub-expression",
                        "Closing parenthesis ')'",
                        "Expression -> Grouping",
                        "Add matching ')' to close the grouped expression",
                    ));
                }
                Ok(val)
            }

            _ => Err(self.error(
                "ERR_SYNTAX_UNEXPECTED_TOKEN",
                "Unexpected token encountered in expression",
                "Literal value, variable ($var), hardware handle (#h), or intrinsic call (@fn)",
                "Expression -> Primary",
                "Replace invalid token with a valid variable name, number, or expression",
            )),
        }
    }
}
