use crate::types::*;

#[derive(Debug, Clone)]
pub struct RiskConfig {
    pub max_notional: i64,
    pub max_position: i64,
    pub fat_finger_pct: f64,
    pub margin_limit: f64,
    pub allow_short: bool,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_notional: 1_000_000 * PRICE_SCALE,
            max_position: 10_000,
            fat_finger_pct: 0.05,
            margin_limit: 0.8,
            allow_short: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AccountRisk {
    pub position: i64,
    pub margin_used: f64,
}

#[derive(Debug)]
pub struct RiskEngine {
    pub config: RiskConfig,
    pub accounts: std::collections::HashMap<AccountId, AccountRisk>,
}

impl RiskEngine {
    pub fn new(config: RiskConfig) -> Self {
        Self {
            config,
            accounts: std::collections::HashMap::new(),
        }
    }

    #[inline]
    pub fn check_order(
        &mut self,
        order: &Order,
        best_bid: Option<Price>,
        best_ask: Option<Price>,
    ) -> RiskResult {
        if !self.check_notional(order) {
            return self.rejected(RejectReason::MaxNotionalExceeded);
        }
        if !self.check_position(order) {
            return self.rejected(RejectReason::MaxPositionExceeded);
        }
        if !self.check_fat_finger(order, best_bid, best_ask) {
            return self.rejected(RejectReason::FatFingerPrice);
        }
        if !self.check_margin(order) {
            return self.rejected(RejectReason::MarginExceeded);
        }
        if !self.check_short(order) {
            return self.rejected(RejectReason::ShortNotAllowed);
        }
        RiskResult::Approved
    }

    #[cold]
    #[inline(always)]
    fn rejected(&self, r: RejectReason) -> RiskResult {
        RiskResult::Rejected(r)
    }

    #[inline]
    fn check_notional(&self, order: &Order) -> bool {
        let notional = order.price.0 * order.qty.0 as i64;
        notional <= self.config.max_notional
    }

    fn check_position(&self, order: &Order) -> bool {
        let account = self.accounts.get(&order.account);
        let current_pos = account.map_or(0, |a| a.position);
        let delta = match order.side {
            Side::Bid => order.qty.0 as i64,
            Side::Ask => -(order.qty.0 as i64),
        };
        (current_pos + delta).abs() <= self.config.max_position
    }

    fn check_fat_finger(
        &self,
        order: &Order,
        best_bid: Option<Price>,
        best_ask: Option<Price>,
    ) -> bool {
        if order.order_type == OrderType::Market || order.order_type == OrderType::IOC {
            return true;
        }
        let mid = match (best_bid, best_ask) {
            (Some(bid), Some(ask)) => Price::midpoint(bid, ask),
            _ => return true,
        };
        if mid.0 == 0 {
            return true;
        }
        let diff = (order.price.0 - mid.0).abs() as f64 / mid.0 as f64;
        diff <= self.config.fat_finger_pct
    }

    fn check_margin(&self, order: &Order) -> bool {
        let account = self.accounts.get(&order.account);
        let used = account.map_or(0.0, |a| a.margin_used);
        let notional = order.price.0 * order.qty.0 as i64;
        let total = used + notional as f64 * 0.1;
        total / (self.config.max_notional as f64) <= self.config.margin_limit
    }

    fn check_short(&self, order: &Order) -> bool {
        if order.side == Side::Ask {
            let account = self.accounts.get(&order.account);
            let pos = account.map_or(0, |a| a.position);
            if pos <= 0 && !self.config.allow_short {
                return false;
            }
        }
        true
    }

    pub fn apply_fill(&mut self, account: AccountId, side: Side, qty: Qty) {
        let entry = self.accounts.entry(account).or_default();
        match side {
            Side::Bid => entry.position += qty.0 as i64,
            Side::Ask => entry.position -= qty.0 as i64,
        }
    }
}
