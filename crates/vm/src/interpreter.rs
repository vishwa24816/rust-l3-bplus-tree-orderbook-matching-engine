use primitive_types::U256;
use journal::{StateJournal, Address};
use crate::opcodes::gas_cost;
use crate::state::{ExecutionState, ExecutionError, ExecMode};

pub struct ExecutionContext {
    pub code: Vec<u8>,
    pub sender: Address,
    pub value: u128,
    pub journal: StateJournal,
}

#[derive(Debug)]
pub struct ExecutionResult {
    pub success: bool,
    pub gas_used: u64,
    pub output: Vec<u8>,
    pub journal: StateJournal,
}

#[inline(always)]
pub fn execute(ctx: ExecutionContext, gas_limit: u64) -> Result<ExecutionResult, ExecutionError> {
    let mut state = ExecutionState::new(ExecMode::Evm, gas_limit);
    let mut journal = ctx.journal;

    loop {
        if state.pc >= ctx.code.len() { break; }
        let op = ctx.code[state.pc];
        state.pc += 1;
        state.deduct_gas(gas_cost(op))?;

        match op {
            0x00 => break,
            0xf3 => {
                let offset = state.pop()?.as_u64() as usize;
                let size = state.pop()?.as_u64() as usize;
                state.ensure_memory(offset, size);
                return Ok(ExecutionResult { success: true, gas_used: gas_limit - state.gas_remaining, output: state.memory[offset..offset+size].to_vec(), journal });
            }
            0xfd => {
                return Ok(ExecutionResult { success: false, gas_used: gas_limit - state.gas_remaining, output: Vec::new(), journal });
            }

            0x01 => { let a = state.pop()?; let b = state.pop()?; state.push(a.overflowing_add(b).0)?; }
            0x02 => { let a = state.pop()?; let b = state.pop()?; state.push(a.overflowing_mul(b).0)?; }
            0x03 => { let a = state.pop()?; let b = state.pop()?; state.push(a.overflowing_sub(b).0)?; }
            0x04 => { let a = state.pop()?; let b = state.pop()?; state.push(if b.is_zero() { U256::zero() } else { a / b })?; }
            0x05 => { let a = state.pop()?; let b = state.pop()?; state.push(if b.is_zero() { U256::zero() } else { a / b })?; }
            0x06 => { let a = state.pop()?; let b = state.pop()?; state.push(if b.is_zero() { U256::zero() } else { a % b })?; }
            0x07 => { let a = state.pop()?; let b = state.pop()?; state.push(if b.is_zero() { U256::zero() } else { a % b })?; }
            0x08 => { let a = state.pop()?; let b = state.pop()?; let m = state.pop()?; state.push(if m.is_zero() { U256::zero() } else { (a + b) % m })?; }
            0x09 => { let a = state.pop()?; let b = state.pop()?; let m = state.pop()?; state.push(if m.is_zero() { U256::zero() } else { (a * b) % m })?; }
            0x0a => { let base = state.pop()?; let exp = state.pop()?; state.push(base.pow(exp))?; }
            0x0b => {
                let b = state.pop()?; let x = state.pop()?;
                state.push(if b < U256::from(32) {
                    let bit = U256::from(1) << (b * 8 + 7);
                    if x & bit != U256::zero() { x | ((!U256::zero()) << (b * 8 + 8)) } else { x & ((!U256::zero()) >> ((256u32 - b.as_u32() * 8 - 8) as usize)) }
                } else { x })?;
            }

            0x10 => { let a = state.pop()?; let b = state.pop()?; state.push(U256::from((a < b) as u64))?; }
            0x11 => { let a = state.pop()?; let b = state.pop()?; state.push(U256::from((a > b) as u64))?; }
            0x12 => { let a = state.pop()?; let b = state.pop()?; state.push(U256::from((a < b) as u64))?; }
            0x13 => { let a = state.pop()?; let b = state.pop()?; state.push(U256::from((a > b) as u64))?; }
            0x14 => { let a = state.pop()?; let b = state.pop()?; state.push(U256::from((a == b) as u64))?; }
            0x15 => { let a = state.pop()?; state.push(U256::from(a.is_zero() as u64))?; }
            0x16 => { let a = state.pop()?; let b = state.pop()?; state.push(a & b)?; }
            0x17 => { let a = state.pop()?; let b = state.pop()?; state.push(a | b)?; }
            0x18 => { let a = state.pop()?; let b = state.pop()?; state.push(a ^ b)?; }
            0x19 => { let a = state.pop()?; state.push(!a)?; }
            0x1a => {
                let i = state.pop()?; let x = state.pop()?;
                state.push(if i < U256::from(32) { U256::from((x >> (248 - i.as_u64() * 8)).as_u64() & 0xff) } else { U256::zero() })?;
            }
            0x1b => { let shift = state.pop()?; let val = state.pop()?; state.push(val << shift.as_u64())?; }
            0x1c => { let shift = state.pop()?; let val = state.pop()?; state.push(val >> shift.as_u64())?; }
            0x1d => { let shift = state.pop()?; let val = state.pop()?; state.push(val >> shift.as_u64())?; }

            0x20 => {
                let offset = state.pop()?.as_u64() as usize;
                let size = state.pop()?.as_u64() as usize;
                state.ensure_memory(offset, size);
                use sha2::{Sha256, Digest};
                let hash = Sha256::digest(&state.memory[offset..offset+size]);
                let mut val = [0u8; 32]; val.copy_from_slice(&hash);
                state.push(U256::from_big_endian(&val))?;
            }

            0x50 => { state.pop()?; }
            0x51 => {
                let offset = state.pop()?.as_u64() as usize;
                state.ensure_memory(offset, 32);
                let mut val = [0u8; 32]; val.copy_from_slice(&state.memory[offset..offset+32]);
                state.push(U256::from_big_endian(&val))?;
            }
            0x52 => {
                let offset = state.pop()?.as_u64() as usize;
                let val = state.pop()?;
                state.ensure_memory(offset, 32);
                let mut bytes = [0u8; 32]; val.to_big_endian(&mut bytes);
                state.memory[offset..offset+32].copy_from_slice(&bytes);
            }
            0x53 => {
                let offset = state.pop()?.as_u64() as usize;
                let val = state.pop()?;
                state.ensure_memory(offset, 1);
                state.memory[offset] = val.as_u64() as u8;
            }
            0x54 => {
                let slot_val = state.pop()?;
                let mut slot = [0u8; 32]; slot_val.to_big_endian(&mut slot);
                let key = journal::StorageKey { address: ctx.sender, slot };
                let val = journal.writes().get(&key).copied().unwrap_or([0u8; 32]);
                state.push(U256::from_big_endian(&val))?;
            }
            0x55 => {
                let slot_val = state.pop()?;
                let val = state.pop()?;
                let mut slot = [0u8; 32]; slot_val.to_big_endian(&mut slot);
                let mut value_bytes = [0u8; 32]; val.to_big_endian(&mut value_bytes);
                journal.record_write(ctx.sender, slot, value_bytes);
            }
            0x56 => {
                let dest = state.pop()?.as_u64() as usize;
                if dest >= ctx.code.len() || ctx.code[dest] != 0x5b { return Err(ExecutionError::InvalidJumpDest(dest as u16)); }
                state.pc = dest;
            }
            0x57 => {
                let dest = state.pop()?.as_u64() as usize;
                let cond = state.pop()?;
                if !cond.is_zero() {
                    if dest >= ctx.code.len() || ctx.code[dest] != 0x5b { return Err(ExecutionError::InvalidJumpDest(dest as u16)); }
                    state.pc = dest;
                }
            }
            0x58 => { state.push(U256::from((state.pc - 1) as u64))?; }
            0x59 => { state.push(U256::from(state.memory.len() as u64))?; }
            0x5a => { state.push(U256::from(state.gas_remaining))?; }
            0x5b => {}

            0x60..=0x7f => {
                let n = (op - 0x60 + 1) as usize;
                if state.pc + n > ctx.code.len() {
                    return Err(ExecutionError::UnexpectedEOF(n, ctx.code.len() - state.pc));
                }
                let mut val_bytes = [0u8; 32];
                val_bytes[32-n..].copy_from_slice(&ctx.code[state.pc..state.pc+n]);
                state.pc += n;
                state.push(U256::from_big_endian(&val_bytes))?;
            }
            0x80..=0x8f => { let n = (op - 0x80) as usize; let val = state.peek(n)?; state.push(val)?; }
            0x90..=0x9f => {
                let n = (op - 0x90 + 1) as usize;
                let a = state.peek(0)?; let b = state.peek(n)?;
                state.stack[state.stack_top - 1] = b;
                state.stack[state.stack_top - 1 - n] = a;
            }
            0xa0..=0xa4 => {
                let n = (op - 0xa0) as usize;
                let _offset = state.pop()?.as_u64() as usize;
                let _size = state.pop()?.as_u64() as usize;
                for _ in 0..n { state.pop()?; }
            }

            _ => return Err(ExecutionError::InvalidOpcode(op)),
        }
    }

    Ok(ExecutionResult { success: true, gas_used: gas_limit - state.gas_remaining, output: Vec::new(), journal })
}
