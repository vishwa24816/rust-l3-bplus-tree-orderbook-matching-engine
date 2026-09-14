use journal::StateJournal;

#[derive(Debug, thiserror::Error)]
pub enum SvmError {
    #[error("halted")]
    Halted,
    #[error("out of instructions")]
    OutOfInstructions,
    #[error("invalid opcode: {0:#04x}")]
    InvalidOpcode(u8),
    #[error("out of gas")]
    OutOfGas,
    #[error("account not found")]
    AccountNotFound,
}

pub struct SvmState {
    pub regs: [u64; 13], // r0-r12, r0 = return value
    pub pc: usize,
    pub gas_remaining: u64,
    pub memory: Vec<u8>,
}

impl SvmState {
    pub fn new(gas_limit: u64) -> Self {
        Self { regs: [0u64; 13], pc: 0, gas_remaining: gas_limit, memory: Vec::with_capacity(4096) }
    }

    #[inline(always)]
    fn read_reg(code: &[u8], pc: &mut usize) -> Result<usize, SvmError> {
        if *pc >= code.len() { return Err(SvmError::OutOfInstructions); }
        let r = code[*pc] as usize;
        *pc += 1;
        if r >= 13 { return Err(SvmError::InvalidOpcode(0xff)); }
        Ok(r)
    }

    #[inline(always)]
    fn read_u16(code: &[u8], pc: &mut usize) -> Result<u16, SvmError> {
        if *pc + 2 > code.len() { return Err(SvmError::OutOfInstructions); }
        let val = u16::from_le_bytes([code[*pc], code[*pc + 1]]);
        *pc += 2;
        Ok(val)
    }

    #[inline(always)]
    fn read_u64(code: &[u8], pc: &mut usize) -> Result<u64, SvmError> {
        if *pc + 8 > code.len() { return Err(SvmError::OutOfInstructions); }
        let val = u64::from_le_bytes(code[*pc..*pc + 8].try_into().unwrap());
        *pc += 8;
        Ok(val)
    }

    fn ensure_memory(&mut self, offset: usize, size: usize) {
        let needed = offset + size;
        if needed > self.memory.len() { self.memory.resize(needed, 0); }
    }
}

#[derive(Debug)]
pub struct SvmResult {
    pub gas_used: u64,
    pub journal: StateJournal,
}

// Opcodes — flat enum, no dynamic dispatch
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum Opcode {
    Halt        = 0x00,
    Mov         = 0x01,
    MovImm      = 0x02,
    Add         = 0x03,
    Sub         = 0x04,
    Mul         = 0x05,
    Div         = 0x06,
    And         = 0x07,
    Or          = 0x08,
    Xor         = 0x09,
    Not         = 0x0a,
    Shl         = 0x0b,
    Shr         = 0x0c,
    Load        = 0x10,
    Store       = 0x11,
    ReadAccount = 0x12,
    WriteAccount= 0x13,
    Jmp         = 0x20,
    Jz          = 0x21,
    Jnz         = 0x22,
    Ret         = 0x30,
}

pub fn svm_execute(code: &[u8], journal: StateJournal, gas_limit: u64) -> Result<SvmResult, SvmError> {
    let mut state = SvmState::new(gas_limit);
    let mut journal = journal;

    loop {
        if state.pc >= code.len() { break; }
        let op = code[state.pc];
        state.pc += 1;
        state.gas_remaining = state.gas_remaining.checked_sub(1).ok_or(SvmError::OutOfGas)?;

        match op {
            0x00 => break, // HALT

            // MOV rd, rs
            0x01 => {
                let rd = SvmState::read_reg(code, &mut state.pc)?;
                let rs = SvmState::read_reg(code, &mut state.pc)?;
                state.regs[rd] = state.regs[rs];
            }

            // MOV_IMM rd, imm64
            0x02 => {
                let rd = SvmState::read_reg(code, &mut state.pc)?;
                let imm = SvmState::read_u64(code, &mut state.pc)?;
                state.regs[rd] = imm;
            }

            // ADD rd, rs1, rs2
            0x03 => {
                let rd = SvmState::read_reg(code, &mut state.pc)?;
                let rs1 = SvmState::read_reg(code, &mut state.pc)?;
                let rs2 = SvmState::read_reg(code, &mut state.pc)?;
                state.regs[rd] = state.regs[rs1].wrapping_add(state.regs[rs2]);
            }

            // SUB rd, rs1, rs2
            0x04 => {
                let rd = SvmState::read_reg(code, &mut state.pc)?;
                let rs1 = SvmState::read_reg(code, &mut state.pc)?;
                let rs2 = SvmState::read_reg(code, &mut state.pc)?;
                state.regs[rd] = state.regs[rs1].wrapping_sub(state.regs[rs2]);
            }

            // MUL rd, rs1, rs2
            0x05 => {
                let rd = SvmState::read_reg(code, &mut state.pc)?;
                let rs1 = SvmState::read_reg(code, &mut state.pc)?;
                let rs2 = SvmState::read_reg(code, &mut state.pc)?;
                state.regs[rd] = state.regs[rs1].wrapping_mul(state.regs[rs2]);
            }

            // DIV rd, rs1, rs2
            0x06 => {
                let rd = SvmState::read_reg(code, &mut state.pc)?;
                let rs1 = SvmState::read_reg(code, &mut state.pc)?;
                let rs2 = SvmState::read_reg(code, &mut state.pc)?;
                state.regs[rd] = if state.regs[rs2] == 0 { 0 } else { state.regs[rs1] / state.regs[rs2] };
            }

            // AND rd, rs1, rs2
            0x07 => {
                let rd = SvmState::read_reg(code, &mut state.pc)?;
                let rs1 = SvmState::read_reg(code, &mut state.pc)?;
                let rs2 = SvmState::read_reg(code, &mut state.pc)?;
                state.regs[rd] = state.regs[rs1] & state.regs[rs2];
            }

            // OR rd, rs1, rs2
            0x08 => {
                let rd = SvmState::read_reg(code, &mut state.pc)?;
                let rs1 = SvmState::read_reg(code, &mut state.pc)?;
                let rs2 = SvmState::read_reg(code, &mut state.pc)?;
                state.regs[rd] = state.regs[rs1] | state.regs[rs2];
            }

            // XOR rd, rs1, rs2
            0x09 => {
                let rd = SvmState::read_reg(code, &mut state.pc)?;
                let rs1 = SvmState::read_reg(code, &mut state.pc)?;
                let rs2 = SvmState::read_reg(code, &mut state.pc)?;
                state.regs[rd] = state.regs[rs1] ^ state.regs[rs2];
            }

            // NOT rd, rs
            0x0a => {
                let rd = SvmState::read_reg(code, &mut state.pc)?;
                let rs = SvmState::read_reg(code, &mut state.pc)?;
                state.regs[rd] = !state.regs[rs];
            }

            // SHL rd, rs, imm8
            0x0b => {
                let rd = SvmState::read_reg(code, &mut state.pc)?;
                let rs = SvmState::read_reg(code, &mut state.pc)?;
                let shift = SvmState::read_reg(code, &mut state.pc)? as u32;
                state.regs[rd] = state.regs[rs] << shift;
            }

            // SHR rd, rs, imm8
            0x0c => {
                let rd = SvmState::read_reg(code, &mut state.pc)?;
                let rs = SvmState::read_reg(code, &mut state.pc)?;
                let shift = SvmState::read_reg(code, &mut state.pc)? as u32;
                state.regs[rd] = state.regs[rs] >> shift;
            }

            // LOAD rd, [addr_reg]
            0x10 => {
                let rd = SvmState::read_reg(code, &mut state.pc)?;
                let addr_r = SvmState::read_reg(code, &mut state.pc)?;
                let offset = state.regs[addr_r] as usize;
                let size = 8; // always load 8 bytes (u64)
                state.ensure_memory(offset, size);
                let val = u64::from_le_bytes(state.memory[offset..offset + 8].try_into().unwrap());
                state.regs[rd] = val;
            }

            // STORE [addr_reg], rs
            0x11 => {
                let addr_r = SvmState::read_reg(code, &mut state.pc)?;
                let rs = SvmState::read_reg(code, &mut state.pc)?;
                let offset = state.regs[addr_r] as usize;
                state.ensure_memory(offset, 8);
                state.memory[offset..offset + 8].copy_from_slice(&state.regs[rs].to_le_bytes());
            }

            // READ_ACCOUNT rd, addr_reg — load first8 bytes of account data into rd
            0x12 => {
                let rd = SvmState::read_reg(code, &mut state.pc)?;
                let addr_r = SvmState::read_reg(code, &mut state.pc)?;
                let mut addr = [0u8; 20];
                let bytes = state.regs[addr_r].to_le_bytes();
                addr[..8].copy_from_slice(&bytes);
                match journal.get_account_data(&addr) {
                    Some(data) => {
                        let mut buf = [0u8; 8];
                        let len = data.len().min(8);
                        buf[..len].copy_from_slice(&data[..len]);
                        state.regs[rd] = u64::from_le_bytes(buf);
                    }
                    None => state.regs[rd] = 0,
                }
            }

            // WRITE_ACCOUNT addr_reg, rs — write rd's value as account data
            0x13 => {
                let addr_r = SvmState::read_reg(code, &mut state.pc)?;
                let rs = SvmState::read_reg(code, &mut state.pc)?;
                let mut addr = [0u8; 20];
                let bytes = state.regs[addr_r].to_le_bytes();
                addr[..8].copy_from_slice(&bytes);
                journal.set_account_data(addr, state.regs[rs].to_le_bytes().to_vec());
            }

            // JMP offset (i16 relative)
            0x20 => {
                let offset = SvmState::read_u16(code, &mut state.pc)? as i16;
                state.pc = (state.pc as isize + offset as isize) as usize;
            }

            // JZ rs, offset
            0x21 => {
                let rs = SvmState::read_reg(code, &mut state.pc)?;
                let offset = SvmState::read_u16(code, &mut state.pc)? as i16;
                if state.regs[rs] == 0 {
                    state.pc = (state.pc as isize + offset as isize) as usize;
                }
            }

            // JNZ rs, offset
            0x22 => {
                let rs = SvmState::read_reg(code, &mut state.pc)?;
                let offset = SvmState::read_u16(code, &mut state.pc)? as i16;
                if state.regs[rs] != 0 {
                    state.pc = (state.pc as isize + offset as isize) as usize;
                }
            }

            // RET — jump to r0 (return value as address)
            0x30 => {
                state.pc = state.regs[0] as usize;
            }

            _ => return Err(SvmError::InvalidOpcode(op)),
        }
    }

    Ok(SvmResult { gas_used: gas_limit - state.gas_remaining, journal })
}
