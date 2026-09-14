use std::collections::HashMap;
use crate::engine::MatchingEngine;
use crate::types::*;

pub struct MatchingRegistry {
    engines: HashMap<Symbol, MatchingEngine>,
}

impl MatchingRegistry {
    pub fn new() -> Self {
        Self {
            engines: HashMap::new(),
        }
    }

    pub fn register(&mut self, symbol: Symbol, engine: MatchingEngine) {
        self.engines.insert(symbol, engine);
    }

    pub fn get(&self, symbol: Symbol) -> Option<&MatchingEngine> {
        self.engines.get(&symbol)
    }

    pub fn get_mut(&mut self, symbol: Symbol) -> Option<&mut MatchingEngine> {
        self.engines.get_mut(&symbol)
    }

    pub fn submit_order(
        &mut self,
        symbol: Symbol,
        order: Order,
    ) -> Result<Vec<Trade>, RejectReason> {
        let engine = self
            .engines
            .get_mut(&symbol)
            .ok_or(RejectReason::MaxNotionalExceeded)?;
        engine.submit_order(order)
    }

    pub fn cancel_order(&mut self, symbol: Symbol, id: OrderId) -> Option<Order> {
        self.engines.get_mut(&symbol)?.cancel_order(id)
    }

    pub fn symbols(&self) -> Vec<Symbol> {
        self.engines.keys().copied().collect()
    }
}

impl Default for MatchingRegistry {
    fn default() -> Self {
        Self::new()
    }
}
