//! px64 Virtual Register Machine Disassembler

use crate::error::CompileError;
use crate::isa::*;

/// Disassemble px64 binary bytecode into a formatter/writer with default filename.
pub fn disassemble_px64<W: core::fmt::Write>(bin: &[u8], w: W) -> Result<(), CompileError> {
    disassemble_px64_with_filename(bin, "binary", w)
}

/// Disassemble px64 binary bytecode into a formatter/writer with custom filename.
pub fn disassemble_px64_with_filename<W: core::fmt::Write>(
    bin: &[u8],
    filename: &str,
    mut w: W,
) -> Result<(), CompileError> {
    if bin.len() >= PX64_HEADER_SIZE && &bin[0..4] == &PX64_BIN_MAGIC {
        let version = u16::from_be_bytes([bin[4], bin[5]]);
        let code_len = u16::from_be_bytes([bin[6], bin[7]]) as usize;
        let str_pool_len = u16::from_be_bytes([bin[8], bin[9]]) as usize;
        let const_count = u16::from_be_bytes([bin[10], bin[11]]) as usize;
        let num_regs = u16::from_be_bytes([bin[12], bin[13]]);

        writeln!(w, "=== [px64 Virtual Register Machine Disassembly] {} ===", filename)?;
        writeln!(w, "NOTE: Registers ($rax..$r15, #f0..#f3) are px64 VM virtual registers, not host CPU GPRs.")?;
        writeln!(
            w,
            "Magic: PX64 | Version: {} | Code: {} B | Registers: {} GPRs+HW | StringPool: {} B | ConstPool: {} entries",
            version, code_len, num_regs, str_pool_len, const_count
        )?;
        writeln!(w, "OFFSET  HEX          INSTRUCTION  OPERANDS (px64 Virtual Registers)")?;
        writeln!(w, "---------------------------------------------------------------")?;

        let const_start = PX64_HEADER_SIZE + code_len + str_pool_len;
        let const_bytes = const_count * 8;
        let mut const_pool = [0i64; 64];
        if bin.len() >= const_start + const_bytes {
            let const_raw = &bin[const_start..const_start + const_bytes];
            for i in 0..core::cmp::min(const_count, 64) {
                let b = &const_raw[i * 8..(i + 1) * 8];
                const_pool[i] = i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
            }
        }

        let code_end = core::cmp::min(PX64_HEADER_SIZE + code_len, bin.len());
        let code = &bin[PX64_HEADER_SIZE..code_end];
        let mut ip = 0;
        while ip + 4 <= code.len() {
            let op_ip = ip;
            let op = code[ip];
            let rd = code[ip + 1];
            let rs1 = code[ip + 2];
            let rs2 = code[ip + 3];
            let imm16 = u16::from_be_bytes([rs1, rs2]);
            let rd_str = px64_reg_name(rd);
            let rs1_str = px64_reg_name(rs1);
            let rs2_str = px64_reg_name(rs2);
            ip += 4;

            match op {
                PX64_OP_NOP => {
                    writeln!(w, "{:04x}:   00 00 00 00  NOP", op_ip)?;
                }
                PX64_OP_MOV_IMM => {
                    writeln!(w, "{:04x}:   01 {:02x} {:02x} {:02x}  MOV          {}, {}", op_ip, rd, rs1, rs2, rd_str, imm16)?;
                }
                PX64_OP_MOV_REG => {
                    writeln!(w, "{:04x}:   02 {:02x} {:02x} 00  MOV          {}, {}", op_ip, rd, rs1, rd_str, rs1_str)?;
                }
                PX64_OP_MOV_STR => {
                    writeln!(w, "{:04x}:   03 {:02x} {:02x} {:02x}  MOVS         {}, str[offset:{}, len:{}]", op_ip, rd, rs1, rs2, rd_str, rs1, rs2)?;
                }
                PX64_OP_ADD => {
                    writeln!(w, "{:04x}:   04 {:02x} {:02x} {:02x}  ADD          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_SUB => {
                    writeln!(w, "{:04x}:   05 {:02x} {:02x} {:02x}  SUB          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_MUL => {
                    writeln!(w, "{:04x}:   06 {:02x} {:02x} {:02x}  MUL          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_DIV => {
                    writeln!(w, "{:04x}:   07 {:02x} {:02x} {:02x}  DIV          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_MOD => {
                    writeln!(w, "{:04x}:   08 {:02x} {:02x} {:02x}  MOD          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_CMP_EQ => {
                    writeln!(w, "{:04x}:   09 {:02x} {:02x} {:02x}  CMPEQ        {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_CMP_NE => {
                    writeln!(w, "{:04x}:   0a {:02x} {:02x} {:02x}  CMPNE        {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_CMP_LT => {
                    writeln!(w, "{:04x}:   0b {:02x} {:02x} {:02x}  CMPLT        {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_CMP_LE => {
                    writeln!(w, "{:04x}:   0c {:02x} {:02x} {:02x}  CMPLE        {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_CMP_GT => {
                    writeln!(w, "{:04x}:   0d {:02x} {:02x} {:02x}  CMPGT        {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_CMP_GE => {
                    writeln!(w, "{:04x}:   0e {:02x} {:02x} {:02x}  CMPGE        {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_JMP => {
                    writeln!(w, "{:04x}:   0f 00 {:02x} {:02x}  JMP          0x{:04x}", op_ip, rs1, rs2, imm16)?;
                }
                PX64_OP_JZ => {
                    writeln!(w, "{:04x}:   10 {:02x} {:02x} {:02x}  JZ           {}, 0x{:04x}", op_ip, rd, rs1, rs2, rd_str, imm16)?;
                }
                PX64_OP_JNZ => {
                    writeln!(w, "{:04x}:   11 {:02x} {:02x} {:02x}  JNZ          {}, 0x{:04x}", op_ip, rd, rs1, rs2, rd_str, imm16)?;
                }
                PX64_OP_CALL_NAT => {
                    let func_name = px64_native_name(rs1);
                    writeln!(w, "{:04x}:   12 {:02x} {:02x} {:02x}  CALL_NAT     {} = {}({})", op_ip, rd, rs1, rs2, rd_str, func_name, rs2_str)?;
                }
                PX64_OP_WITHIN_START => {
                    writeln!(w, "{:04x}:   13 {:02x} 00 00  WITHIN_START budget:{}", op_ip, rd, rd_str)?;
                }
                PX64_OP_WITHIN_END => {
                    writeln!(w, "{:04x}:   14 00 00 00  WITHIN_END", op_ip)?;
                }
                PX64_OP_DROP => {
                    writeln!(w, "{:04x}:   15 00 00 00  DROP", op_ip)?;
                }
                PX64_OP_HALT => {
                    writeln!(w, "{:04x}:   16 00 00 00  HALT", op_ip)?;
                }
                PX64_OP_LDC => {
                    let const_idx = imm16 as usize;
                    if const_idx < const_count {
                        writeln!(w, "{:04x}:   17 {:02x} {:02x} {:02x}  LDC          {}, const[{}] ({})", op_ip, rd, rs1, rs2, rd_str, imm16, const_pool[const_idx])?;
                    } else {
                        writeln!(w, "{:04x}:   17 {:02x} {:02x} {:02x}  LDC          {}, const[{}] (<out of bounds>)", op_ip, rd, rs1, rs2, rd_str, imm16)?;
                    }
                }
                PX64_OP_ADDI => {
                    writeln!(w, "{:04x}:   18 {:02x} {:02x} {:02x}  ADDI         {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2)?;
                }
                PX64_OP_SUBI => {
                    writeln!(w, "{:04x}:   19 {:02x} {:02x} {:02x}  SUBI         {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2)?;
                }
                PX64_OP_AND => {
                    writeln!(w, "{:04x}:   1a {:02x} {:02x} {:02x}  AND          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_OR => {
                    writeln!(w, "{:04x}:   1b {:02x} {:02x} {:02x}  OR           {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_XOR => {
                    writeln!(w, "{:04x}:   1c {:02x} {:02x} {:02x}  XOR          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_SHL => {
                    writeln!(w, "{:04x}:   1d {:02x} {:02x} {:02x}  SHL          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_SHR => {
                    writeln!(w, "{:04x}:   1e {:02x} {:02x} {:02x}  SHR          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                PX64_OP_ARR_DEF => {
                    writeln!(w, "{:04x}:   1f {:02x} {:02x} {:02x}  ARR_DEF      arr[{}], len: {}", op_ip, rd, rs1, rs2, rd, imm16)?;
                }
                PX64_OP_ARR_LOAD => {
                    writeln!(w, "{:04x}:   20 {:02x} {:02x} {:02x}  ARR_LOAD     {}, arr[{}][{}]", op_ip, rd, rs1, rs2, rd_str, rs1, rs2_str)?;
                }
                PX64_OP_ARR_STORE => {
                    writeln!(w, "{:04x}:   21 {:02x} {:02x} {:02x}  ARR_STORE    arr[{}][{}], {}", op_ip, rd, rs1, rs2, rd, rs1_str, rs2_str)?;
                }
                PX64_OP_ASSERT => {
                    writeln!(w, "{:04x}:   22 {:02x} 00 00  ASSERT       {}", op_ip, rd, rd_str)?;
                }
                PX64_OP_CALL => {
                    writeln!(w, "{:04x}:   23 00 {:02x} {:02x}  CALL         0x{:04x}", op_ip, rs1, rs2, imm16)?;
                }
                PX64_OP_RET => {
                    writeln!(w, "{:04x}:   24 00 00 00  RET", op_ip)?;
                }
                PX64_OP_STRUCT_DEF => {
                    writeln!(w, "{:04x}:   25 {:02x} {:02x} 00  STRUCT_DEF   inst[{}], fields: {}", op_ip, rd, rs1, rd, rs1)?;
                }
                PX64_OP_STRUCT_LOAD => {
                    writeln!(w, "{:04x}:   26 {:02x} {:02x} {:02x}  STRUCT_LOAD  {}, inst[{}].f[{}]", op_ip, rd, rs1, rs2, rd_str, rs1, rs2)?;
                }
                PX64_OP_STRUCT_STORE => {
                    writeln!(w, "{:04x}:   27 {:02x} {:02x} {:02x}  STRUCT_STORE inst[{}].f[{}], {}", op_ip, rd, rs1, rs2, rd, rs1, rs2_str)?;
                }
                PX64_OP_TBL_DEF => {
                    writeln!(w, "{:04x}:   28 {:02x} {:02x} {:02x}  TBL_DEF      table[{}], base: {}, len: {}", op_ip, rd, rs1, rs2, rd, rs1, rs2)?;
                }
                PX64_OP_TBL_LOAD => {
                    writeln!(w, "{:04x}:   29 {:02x} {:02x} {:02x}  TBL_LOAD     {}, table[{}][{}]", op_ip, rd, rs1, rs2, rd_str, rs1, rs2_str)?;
                }
                PX64_OP_STREQ => {
                    writeln!(w, "{:04x}:   2a {:02x} {:02x} {:02x}  STREQ        {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str)?;
                }
                _ => {
                    writeln!(w, "{:04x}:   {:02x} {:02x} {:02x} {:02x}  UNKNOWN_OP_0x{:02x}", op_ip, op, rd, rs1, rs2, op)?;
                }
            }
        }
        Ok(())
    } else if bin.len() >= PULSE_HEADER_SIZE && &bin[0..4] == &PULSE_BIN_MAGIC {
        let code_len = u16::from_be_bytes([bin[6], bin[7]]) as usize;
        let str_pool_len = u16::from_be_bytes([bin[8], bin[9]]) as usize;
        writeln!(w, "=== Legacy PulseLang Bytecode Disassembly: {} ===", filename)?;
        writeln!(w, "Magic: PULS | Version: 2 | Code: {} B | StringPool: {} B", code_len, str_pool_len)?;
        writeln!(w, "OFFSET  OPCODE              OPERANDS")?;
        writeln!(w, "---------------------------------------------------")?;
        let code_end = core::cmp::min(PULSE_HEADER_SIZE + code_len, bin.len());
        let code = &bin[PULSE_HEADER_SIZE..code_end];
        let mut ip = 0;
        while ip < code.len() {
            let op_ip = ip;
            let op = code[ip];
            ip += 1;
            match op {
                OP_NOP => writeln!(w, "{:04x}:   OP_NOP", op_ip)?,
                OP_PUSH_CONST => {
                    if ip + 8 <= code.len() {
                        let mut b = [0u8; 8];
                        b.copy_from_slice(&code[ip..ip + 8]);
                        ip += 8;
                        let val = i64::from_be_bytes(b);
                        writeln!(w, "{:04x}:   OP_PUSH_CONST       {}", op_ip, val)?;
                    }
                }
                OP_LOAD_VAR => {
                    if ip < code.len() {
                        let slot = code[ip];
                        ip += 1;
                        writeln!(w, "{:04x}:   OP_LOAD_VAR         slot:{}", op_ip, slot)?;
                    }
                }
                OP_STORE_VAR => {
                    if ip < code.len() {
                        let slot = code[ip];
                        ip += 1;
                        writeln!(w, "{:04x}:   OP_STORE_VAR        slot:{}", op_ip, slot)?;
                    }
                }
                OP_ADD => writeln!(w, "{:04x}:   OP_ADD", op_ip)?,
                OP_SUB => writeln!(w, "{:04x}:   OP_SUB", op_ip)?,
                OP_MUL => writeln!(w, "{:04x}:   OP_MUL", op_ip)?,
                OP_DIV => writeln!(w, "{:04x}:   OP_DIV", op_ip)?,
                OP_MOD => writeln!(w, "{:04x}:   OP_MOD", op_ip)?,
                OP_CMP_EQ => writeln!(w, "{:04x}:   OP_CMP_EQ", op_ip)?,
                OP_CMP_NE => writeln!(w, "{:04x}:   OP_CMP_NE", op_ip)?,
                OP_CMP_LT => writeln!(w, "{:04x}:   OP_CMP_LT", op_ip)?,
                OP_CMP_LE => writeln!(w, "{:04x}:   OP_CMP_LE", op_ip)?,
                OP_CMP_GT => writeln!(w, "{:04x}:   OP_CMP_GT", op_ip)?,
                OP_CMP_GE => writeln!(w, "{:04x}:   OP_CMP_GE", op_ip)?,
                OP_JUMP => {
                    if ip + 2 <= code.len() {
                        let target = u16::from_be_bytes([code[ip], code[ip + 1]]);
                        ip += 2;
                        writeln!(w, "{:04x}:   OP_JUMP             0x{:04x}", op_ip, target)?;
                    }
                }
                OP_JUMP_IF_FALSE => {
                    if ip + 2 <= code.len() {
                        let target = u16::from_be_bytes([code[ip], code[ip + 1]]);
                        ip += 2;
                        writeln!(w, "{:04x}:   OP_JUMP_IF_FALSE    0x{:04x}", op_ip, target)?;
                    }
                }
                OP_CALL_NATIVE => {
                    if ip < code.len() {
                        let id = code[ip];
                        ip += 1;
                        let name = px64_native_name(id);
                        writeln!(w, "{:04x}:   OP_CALL_NATIVE      {} (id:{})", op_ip, name, id)?;
                    }
                }
                OP_WITHIN_START => {
                    if ip + 8 <= code.len() {
                        let mut b = [0u8; 8];
                        b.copy_from_slice(&code[ip..ip + 8]);
                        ip += 8;
                        let us = u64::from_be_bytes(b);
                        writeln!(w, "{:04x}:   OP_WITHIN_START     {} us", op_ip, us)?;
                    }
                }
                OP_WITHIN_END => writeln!(w, "{:04x}:   OP_WITHIN_END", op_ip)?,
                OP_DROP => writeln!(w, "{:04x}:   OP_DROP", op_ip)?,
                OP_PUSH_STR => {
                    if ip + 2 <= code.len() {
                        let offset = code[ip];
                        let len = code[ip + 1];
                        ip += 2;
                        writeln!(w, "{:04x}:   OP_PUSH_STR         offset:{}, len:{}", op_ip, offset, len)?;
                    }
                }
                OP_HALT => writeln!(w, "{:04x}:   OP_HALT", op_ip)?,
                _ => writeln!(w, "{:04x}:   UNKNOWN_OP_{}", op_ip, op)?,
            }
        }
        Ok(())
    } else {
        Err(CompileError::simple(
            "ERR_BINARY_INVALID_MAGIC",
            "Invalid executable binary magic (expected PX64)",
        ))
    }
}

#[cfg(any(feature = "alloc", test))]
pub fn disasm(bin: &[u8]) -> Result<alloc::string::String, CompileError> {
    disasm_with_filename(bin, "binary")
}

#[cfg(any(feature = "alloc", test))]
pub fn disasm_with_filename(bin: &[u8], filename: &str) -> Result<alloc::string::String, CompileError> {
    let mut out = alloc::string::String::new();
    disassemble_px64_with_filename(bin, filename, &mut out)?;
    Ok(out)
}
