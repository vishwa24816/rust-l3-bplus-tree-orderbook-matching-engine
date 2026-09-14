/// Routes bytecode to the correct execution engine.
/// Ponytail: just an enum + match. No trait objects, no dynamic dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionEngine {
    Evm,
    Svm,
}

pub struct EngineRouter;

/// ELF magic bytes: 0x7F 'E' 'L' 'F'
const ELF_MAGIC: [u8; 4] = [0x7f, 0x45, 0x4c, 0x46];

impl EngineRouter {
    /// Detect engine from bytecode prefix.
    /// 4-byte ELF magic = SVM, anything else = EVM.
    /// Ponytail: 4-byte memcmp, no allocation, no parsing.
    pub fn detect(code: &[u8]) -> ExecutionEngine {
        if code.len() >= 4 && code[..4] == ELF_MAGIC { ExecutionEngine::Svm } else { ExecutionEngine::Evm }
    }

    /// Strip ELF header if present, return裸 bytecode.
    pub fn strip_prefix(code: &[u8]) -> &[u8] {
        if code.len() >= 4 && code[..4] == ELF_MAGIC { &code[4..] } else { code }
    }
}
