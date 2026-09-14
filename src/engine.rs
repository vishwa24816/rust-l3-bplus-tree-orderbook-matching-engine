use crate::book::OrderBook;
use crate::risk::RiskEngine;
use crate::types::*;

pub struct MatchingEngine {
    pub book: OrderBook,
    pub risk: RiskEngine,
    pub sequence: u64,
    pub trades: Vec<Trade>,
    pub stp_policy: Option<StpPolicy>,
    pub session_end: u64,
}

impl MatchingEngine {
    pub fn new(risk: RiskEngine) -> Self {
        Self {
            book: OrderBook::new(),
            risk,
            sequence: 0,
            trades: Vec::with_capacity(256),
            stp_policy: None,
            session_end: u64::MAX,
        }
    }

    pub fn with_stp(risk: RiskEngine, policy: StpPolicy) -> Self {
        Self {
            book: OrderBook::new(),
            risk,
            sequence: 0,
            trades: Vec::with_capacity(256),
            stp_policy: Some(policy),
            session_end: u64::MAX,
        }
    }

    pub fn with_session(risk: RiskEngine, session_end: u64) -> Self {
        Self {
            book: OrderBook::new(),
            risk,
            sequence: 0,
            trades: Vec::with_capacity(256),
            stp_policy: None,
            session_end,
        }
    }

    #[inline]
    pub fn submit_order(&mut self, mut order: Order) -> Result<Vec<Trade>, RejectReason> {
        // DAY TIF: reject if session already ended
        if order.tif == TimeInForce::DAY && self.sequence >= self.session_end {
            return Err(RejectReason::MaxNotionalExceeded);
        }

        let best_bid = self.book.best_bid();
        let best_ask = self.book.best_ask();

        match self.risk.check_order(&order, best_bid, best_ask) {
            RiskResult::Approved => {}
            RiskResult::Rejected(r) => return Err(r),
        }

        self.sequence += 1;
        order.timestamp = self.sequence;

        let mut unfilled = order.clone();
        let trades = self.book.match_order(&mut unfilled, self.stp_policy);

        if order.order_type == OrderType::FOK {
            let filled_qty = order.qty.0 - unfilled.remaining.0;
            #[cold]
            fn fok_rejected(trades: &[Trade], book: &mut OrderBook) {
                for t in trades {
                    book.cancel_order(t.taker_id);
                }
            }
            if filled_qty < order.qty.0 {
                fok_rejected(&trades, &mut self.book);
                return Err(RejectReason::MaxNotionalExceeded);
            }
        }

        #[cold]
        fn post_only_rejected() -> RejectReason {
            RejectReason::FatFingerPrice
        }
        if order.order_type == OrderType::PostOnly && unfilled.remaining.0 < order.qty.0 {
            return Err(post_only_rejected());
        }

        if unfilled.remaining.0 > 0
            && order.order_type != OrderType::Market
            && order.order_type != OrderType::IOC
            && order.order_type != OrderType::FOK
        {
            self.book.insert_order(unfilled);
        }

        for trade in &trades {
            self.risk.apply_fill(order.account, order.side, trade.qty);
        }

        self.trades.extend_from_slice(&trades);
        Ok(trades)
    }

    /// Expire all DAY orders resting in the book. Returns count of expired orders.
    pub fn expire_day_orders(&mut self) -> usize {
        let mut expired = 0;
        for levels in [&mut self.book.bids, &mut self.book.asks] {
            let prices: Vec<Price> = levels.keys().copied().collect();
            for price in prices {
                if let Some(level) = levels.get_mut(&price) {
                    let before = level.orders.len();
                    level
                        .orders
                        .retain(|o| !(o.tif == TimeInForce::DAY && o.timestamp < self.session_end));
                    let removed = before - level.orders.len();
                    expired += removed;
                    // Recalculate total_qty
                    level.total_qty = Qty(level.orders.iter().map(|o| o.remaining.0).sum());
                }
                // Remove empty levels
                if levels.get(&price).is_some_and(|l| l.orders.is_empty()) {
                    levels.remove(&price);
                }
            }
        }
        expired
    }

    #[inline]
    pub fn cancel_order(&mut self, id: OrderId) -> Option<Order> {
        self.book.cancel_order(id)
    }
}
