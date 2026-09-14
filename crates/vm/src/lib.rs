pub mod interpreter;
pub mod opcodes;
pub mod state;
pub mod svm;

pub use interpreter::{execute, ExecutionContext, ExecutionResult};
pub use state::{ExecutionState, ExecutionError, ExecMode};
pub use svm::{svm_execute, SvmResult, SvmError};

#[cfg(test)]
mod tests {
    use super::*;
    use journal::StateJournal;

    fn evm_run(code: Vec<u8>, gas: u64) -> Result<ExecutionResult, ExecutionError> {
        let ctx = ExecutionContext { code, sender: [0u8; 20], value: 0, journal: StateJournal::new() };
        execute(ctx, gas)
    }

    #[test]
    fn test_evm_stop() {
        let r = evm_run(vec![0x00], 1000).unwrap();
        assert!(r.success);
        assert_eq!(r.gas_used, 0);
    }

    #[test]
    fn test_evm_add() {
        let r = evm_run(vec![0x60, 0x05, 0x60, 0x03, 0x01, 0x00], 1000).unwrap();
        assert!(r.success);
    }

    #[test]
    fn test_evm_push_truncated_eof() {
        // PUSH1 needs 1 data byte — only the opcode is present, no data
        let err = evm_run(vec![0x60], 1000).unwrap_err();
        assert!(matches!(err, ExecutionError::UnexpectedEOF(1, 0)));
    }

    #[test]
    fn test_evm_push2_truncated_eof() {
        // PUSH2 needs 2 data bytes — only 1 available
        let err = evm_run(vec![0x61, 0x00], 1000).unwrap_err();
        assert!(matches!(err, ExecutionError::UnexpectedEOF(2, 1)));
    }

    #[test]
    fn test_evm_stack_underflow() {
        let err = evm_run(vec![0x01, 0x00], 1000).unwrap_err(); // ADD with empty stack
        assert!(matches!(err, ExecutionError::StackUnderflow));
    }

    #[test]
    fn test_evm_invalid_opcode() {
        let err = evm_run(vec![0xfe, 0x00], 1000).unwrap_err();
        assert!(matches!(err, ExecutionError::InvalidOpcode(0xfe)));
    }

    #[test]
    fn test_evm_out_of_gas() {
        let err = evm_run(vec![0x60, 0x05, 0x60, 0x03, 0x01, 0x00], 5).unwrap_err();
        assert!(matches!(err, ExecutionError::OutOfGas));
    }

    #[test]
    fn test_svm_halt() {
        let r = svm_execute(&[0x00], StateJournal::new(), 1000).unwrap();
        assert_eq!(r.gas_used, 1);
    }

    #[test]
    fn test_svm_add() {
        let code = vec![
            0x02, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x02, 0x01, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x03, 0x02, 0x00, 0x01,
            0x00,
        ];
        let r = svm_execute(&code, StateJournal::new(), 1000).unwrap();
        assert_eq!(r.gas_used, 4); // 2 MOV_IMM + ADD + HALT
    }

    #[test]
    fn test_svm_invalid_opcode() {
        let err = svm_execute(&[0xff], StateJournal::new(), 1000).unwrap_err();
        assert!(matches!(err, SvmError::InvalidOpcode(0xff)));
    }

    #[test]
    fn test_svm_out_of_gas() {
        let code = vec![
            0x02, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00,
        ];
        let err = svm_execute(&code, StateJournal::new(), 1).unwrap_err(); // MOV_IMM costs 1, HALT costs 1 = need 2
        assert!(matches!(err, SvmError::OutOfGas));
    }
}
