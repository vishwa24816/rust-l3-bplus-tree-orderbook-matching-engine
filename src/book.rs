use std::collections::{BTreeMap, VecDeque};
use crate::types::*;

const INITIAL_ORDERS_PER_LEVEL: usize = 16;

pub type L1Level = (Price, Qty);
pub type L2Level = (Price, Qty, usize);

#[derive(Debug, Clone)]
pub struct PriceLevel {
    pub price: Price,
    pub orders: VecDeque<Order>,
    pub total_qty: Qty,
}

#[derive(Debug)]
pub struct OrderBook {
    pub bids: BTreeMap<Price, PriceLevel>,
    pub asks: BTreeMap<Price, PriceLevel>,
}

impl OrderBook {
    pub fn new() -> Self {
        // ponytail: BTreeMap doesn't support pre-size hints, but VecDeque inside
        // levels does. Pre-alloc per-level queue; BTreeMap grows organically.
        // Upgrade: flat array + index map if BTree traversal becomes瓶颈.
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }

    #[inline]
    pub fn best_bid(&self) -> Option<Price> {
        self.bids.keys().next_back().copied()
    }

    #[inline]
    pub fn best_ask(&self) -> Option<Price> {
        self.asks.keys().next().copied()
    }

    /// L1: Top of book — best bid/ask with aggregate depth.
    #[inline]
    pub fn l1_bbo(&self) -> (Option<L1Level>, Option<L1Level>) {
        let bid = self.best_bid().map(|p| {
            let level = &self.bids[&p];
            (p, level.total_qty)
        });
        let ask = self.best_ask().map(|p| {
            let level = &self.asks[&p];
            (p, level.total_qty)
        });
        (bid, ask)
    }

    /// L2: Price-level aggregation — top N levels per side.
    pub fn l2_depth(&self, depth: usize) -> (Vec<L2Level>, Vec<L2Level>) {
        let bids: Vec<_> = self
            .bids
            .iter()
            .rev()
            .take(depth)
            .map(|(p, l)| (*p, l.total_qty, l.orders.len()))
            .collect();
        let asks: Vec<_> = self
            .asks
            .iter()
            .take(depth)
            .map(|(p, l)| (*p, l.total_qty, l.orders.len()))
            .collect();
        (bids, asks)
    }

    /// L3: Full order-by-order view for a given price level.
    pub fn l3_orders_at(&self, price: Price, side: Side) -> Vec<&Order> {
        let levels = match side {
            Side::Bid => &self.bids,
            Side::Ask => &self.asks,
        };
        levels
            .get(&price)
            .map(|l| l.orders.iter().collect())
            .unwrap_or_default()
    }

    pub fn insert_order(&mut self, order: Order) {
        let levels = match order.side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        let level = levels.entry(order.price).or_insert_with(|| PriceLevel {
            price: order.price,
            orders: VecDeque::with_capacity(INITIAL_ORDERS_PER_LEVEL),
            total_qty: Qty(0),
        });
        level.total_qty.0 += order.remaining.0;
        level.orders.push_back(order);
    }

    #[inline]
    pub fn cancel_order(&mut self, id: OrderId) -> Option<Order> {
        for levels in [&mut self.bids, &mut self.asks] {
            for (_, level) in levels.range_mut(..) {
                if let Some(pos) = level.orders.iter().position(|o| o.id == id) {
                    let order = level.orders.remove(pos).unwrap();
                    level.total_qty.0 -= order.remaining.0;
                    if level.orders.is_empty() {
                        let price = level.price;
                        levels.remove(&price);
                    }
                    return Some(order);
                }
            }
        }
        None
    }

    /// Cancel order by ID, returning the order and its account for STP lookup.
    #[inline]
    pub fn cancel_order_with_account(&mut self, id: OrderId) -> Option<(Order, AccountId)> {
        for levels in [&mut self.bids, &mut self.asks] {
            for (_, level) in levels.range_mut(..) {
                if let Some(pos) = level.orders.iter().position(|o| o.id == id) {
                    let order = level.orders.remove(pos).unwrap();
                    let account = order.account;
                    level.total_qty.0 -= order.remaining.0;
                    if level.orders.is_empty() {
                        let price = level.price;
                        levels.remove(&price);
                    }
                    return Some((order, account));
                }
            }
        }
        None
    }

    pub fn match_order(&mut self, taker: &mut Order, stp_policy: Option<StpPolicy>) -> Vec<Trade> {
        let mut trades = Vec::with_capacity(8);
        let opposite = match taker.side {
            Side::Bid => &mut self.asks,
            Side::Ask => &mut self.bids,
        };

        let prices: Vec<Price> = opposite.keys().copied().collect();

        for price in prices {
            if taker.remaining.0 == 0 {
                break;
            }
            let crosses = match taker.side {
                Side::Bid => taker.price >= price,
                Side::Ask => taker.price <= price,
            };
            if !crosses && taker.order_type != OrderType::Market {
                break;
            }
            if !crosses && taker.order_type == OrderType::Market {
                continue;
            }

            if let Some(level) = opposite.get_mut(&price) {
                while let Some(maker) = level.orders.front_mut() {
                    if taker.remaining.0 == 0 {
                        break;
                    }

                    // Self-trade prevention: skip same-account orders
                    if let Some(policy) = stp_policy
                        && maker.account == taker.account
                    {
                        match policy {
                            StpPolicy::CancelPassive => {
                                let cancelled = level.orders.pop_front().unwrap();
                                level.total_qty.0 -= cancelled.remaining.0;
                            }
                            StpPolicy::CancelAggressive => {
                                taker.remaining.0 = 0;
                            }
                            StpPolicy::DecrementAndCancel => {
                                let max_fill =
                                    std::cmp::min(maker.remaining.0, taker.remaining.0);
                                let half = max_fill / 2;
                                let maker_fill = if half > 0 { half } else { max_fill };
                                let taker_fill = max_fill - maker_fill;

                                maker.remaining.0 -= maker_fill;
                                taker.remaining.0 -= taker_fill;
                                level.total_qty.0 -= max_fill;

                                if maker_fill > 0 {
                                    trades.push(Trade {
                                        maker_id: maker.id,
                                        taker_id: taker.id,
                                        price: maker.price,
                                        qty: Qty(maker_fill),
                                        timestamp: taker.timestamp,
                                    });
                                }
                                level.orders.pop_front();
                                taker.remaining.0 = 0;
                            }
                        }
                        continue;
                    }

                    let fill_qty = std::cmp::min(maker.remaining.0, taker.remaining.0);
                    maker.remaining.0 -= fill_qty;
                    taker.remaining.0 -= fill_qty;
                    level.total_qty.0 -= fill_qty;

                    trades.push(Trade {
                        maker_id: maker.id,
                        taker_id: taker.id,
                        price: maker.price,
                        qty: Qty(fill_qty),
                        timestamp: taker.timestamp,
                    });

                    if maker.remaining.0 == 0 {
                        level.orders.pop_front();
                    }
                }
                if level.orders.is_empty() {
                    let p = level.price;
                    opposite.remove(&p);
                }
            }
        }

        trades
    }

    pub fn l2_snapshot(&self, depth: usize) -> Vec<(Price, Qty, usize)> {
        let (bids, asks) = self.l2_depth(depth);
        let mut result = bids;
        result.extend(asks);
        result
    }
}

impl Default for OrderBook {
    fn default() -> Self {
        Self::new()
    }
}
