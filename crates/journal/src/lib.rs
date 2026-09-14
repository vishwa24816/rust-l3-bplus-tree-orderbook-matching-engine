use std::collections::HashMap;

pub type Address = [u8; 20];
pub type Slot = [u8; 32];
pub type Value = [u8; 32];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StorageKey {
    pub address: Address,
    pub slot: Slot,
}

#[derive(Debug, Clone)]
pub struct StateJournal {
    writes: HashMap<StorageKey, Value>,
    gas_deductions: Vec<(Address, u64)>,
    // SVM account data — ponytail: one HashMap, no abstraction over "account types"
    accounts: HashMap<Address, Vec<u8>>,
}

impl StateJournal {
    pub fn new() -> Self {
        Self { writes: HashMap::new(), gas_deductions: Vec::new(), accounts: HashMap::new() }
    }

    // EVM
    pub fn record_write(&mut self, address: Address, slot: Slot, value: Value) {
        self.writes.insert(StorageKey { address, slot }, value);
    }

    pub fn record_gas_deduction(&mut self, address: Address, amount: u64) {
        self.gas_deductions.push((address, amount));
    }

    // SVM — ponytail: direct HashMap ops, no AccountInfo wrapper struct
    pub fn set_account_data(&mut self, address: Address, data: Vec<u8>) {
        self.accounts.insert(address, data);
    }

    pub fn get_account_data(&self, address: &Address) -> Option<&[u8]> {
        self.accounts.get(address).map(|v| v.as_slice())
    }

    pub fn accounts(&self) -> &HashMap<Address, Vec<u8>> { &self.accounts }

    pub fn writes(&self) -> &HashMap<StorageKey, Value> { &self.writes }
    pub fn gas_deductions(&self) -> &[(Address, u64)] { &self.gas_deductions }
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty() && self.gas_deductions.is_empty() && self.accounts.is_empty()
    }
}

impl Default for StateJournal {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_is_empty() {
        let j = StateJournal::new();
        assert!(j.is_empty());
    }

    #[test]
    fn test_record_write_and_read() {
        let mut j = StateJournal::new();
        let addr = [1u8; 20];
        let slot = [2u8; 32];
        let val = [42u8; 32];
        j.record_write(addr, slot, val);
        assert!(!j.is_empty());
        assert_eq!(j.writes().get(&StorageKey { address: addr, slot }), Some(&val));
    }

    #[test]
    fn test_gas_deduction() {
        let mut j = StateJournal::new();
        let addr = [1u8; 20];
        j.record_gas_deduction(addr, 21000);
        assert_eq!(j.gas_deductions().len(), 1);
        assert_eq!(j.gas_deductions()[0], (addr, 21000));
    }

    #[test]
    fn test_account_data() {
        let mut j = StateJournal::new();
        let addr = [5u8; 20];
        assert!(j.get_account_data(&addr).is_none());
        j.set_account_data(addr, vec![1, 2, 3]);
        assert_eq!(j.get_account_data(&addr), Some(&[1, 2, 3][..]));
    }

    #[test]
    fn test_overwrite_account_data() {
        let mut j = StateJournal::new();
        let addr = [5u8; 20];
        j.set_account_data(addr, vec![1, 2]);
        j.set_account_data(addr, vec![3, 4, 5]);
        assert_eq!(j.get_account_data(&addr), Some(&[3, 4, 5][..]));
    }

    #[test]
    fn test_rollback_simulation() {
        let mut j = StateJournal::new();
        let addr = [1u8; 20];
        let slot = [0u8; 32];
        j.record_write(addr, slot, [10u8; 32]);
        j.record_gas_deduction(addr, 100);
        j.set_account_data(addr, vec![99]);
        assert!(!j.is_empty());
        // Simulate rollback: create fresh journal
        let j2 = StateJournal::new();
        assert!(j2.is_empty());
        // Original still has data
        assert!(!j.is_empty());
    }
}
