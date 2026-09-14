use primitive_types::U256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecMode {
    Evm,
    Svm,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    #[error("out of gas")]
    OutOfGas,
    #[error("stack overflow")]
    StackOverflow,
    #[error("stack underflow")]
    StackUnderflow,
    #[error("invalid opcode: {0:#04x}")]
    InvalidOpcode(u8),
    #[error("revert")]
    Revert,
    #[error("invalid jump destination: {0}")]
    InvalidJumpDest(u16),
    #[error("unexpected end of code: need {0} bytes, have {1}")]
    UnexpectedEOF(usize, usize),
}

pub struct ExecutionState {
    pub stack: [U256; 1024],
    pub stack_top: usize,
    pub memory: Vec<u8>,
    pub pc: usize,
    pub gas_remaining: u64,
    pub mode: ExecMode,
    pub registers: [u64; 32],
}

impl ExecutionState {
    pub fn new(mode: ExecMode, gas_limit: u64) -> Self {
        Self {
            stack: [U256::zero(); 1024], stack_top: 0,
            memory: Vec::with_capacity(4096), pc: 0,
            gas_remaining: gas_limit, mode, registers: [0u64; 32],
        }
    }

    #[inline(always)]
    pub fn push(&mut self, val: U256) -> Result<(), ExecutionError> {
        if self.stack_top >= 1024 { return Err(ExecutionError::StackOverflow); }
        self.stack[self.stack_top] = val;
        self.stack_top += 1;
        Ok(())
    }

    #[inline(always)]
    pub fn pop(&mut self) -> Result<U256, ExecutionError> {
        if self.stack_top == 0 { return Err(ExecutionError::StackUnderflow); }
        self.stack_top -= 1;
        Ok(self.stack[self.stack_top])
    }

    #[inline(always)]
    pub fn peek(&self, offset: usize) -> Result<U256, ExecutionError> {
        if self.stack_top <= offset { return Err(ExecutionError::StackUnderflow); }
        Ok(self.stack[self.stack_top - 1 - offset])
    }

    #[inline(always)]
    pub fn deduct_gas(&mut self, amount: u64) -> Result<(), ExecutionError> {
        if self.gas_remaining < amount { return Err(ExecutionError::OutOfGas); }
        self.gas_remaining -= amount;
        Ok(())
    }

    pub fn ensure_memory(&mut self, offset: usize, size: usize) {
        let needed = offset + size;
        if needed > self.memory.len() { self.memory.resize(needed, 0); }
    }
}
