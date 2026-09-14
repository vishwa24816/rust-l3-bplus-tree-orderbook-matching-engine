#[cfg(test)]
mod tests {
    use crate::*;

    fn make_order(id: u64, side: Side, price: f64, qty: u64) -> Order {
        Order {
            id: OrderId(id),
            account: AccountId(1),
            side,
            order_type: OrderType::Limit,
            price: Price::from_f64(price),
            qty: Qty(qty),
            remaining: Qty(qty),
            tif: TimeInForce::GTC,
            timestamp: 0,
        }
    }

    fn engine() -> MatchingEngine {
        MatchingEngine::new(RiskEngine::new(RiskConfig::default()))
    }

    #[test]
    fn test_bid_ask_match() {
        let mut e = engine();
        e.submit_order(make_order(1, Side::Bid, 100.0, 10)).unwrap();
        let trades = e.submit_order(make_order(2, Side::Ask, 100.0, 5)).unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].qty, Qty(5));
    }

    #[test]
    fn test_no_cross_no_fill() {
        let mut e = engine();
        e.submit_order(make_order(1, Side::Bid, 99.0, 10)).unwrap();
        let trades = e.submit_order(make_order(2, Side::Ask, 101.0, 10)).unwrap();
        assert!(trades.is_empty());
    }

    #[test]
    fn test_partial_fill() {
        let mut e = engine();
        e.submit_order(make_order(1, Side::Bid, 100.0, 5)).unwrap();
        let trades = e.submit_order(make_order(2, Side::Ask, 100.0, 10)).unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].qty, Qty(5));
        assert_eq!(e.book.best_ask(), Some(Price::from_f64(100.0)));
    }

    #[test]
    fn test_price_time_priority() {
        let mut e = engine();
        e.submit_order(make_order(1, Side::Ask, 100.0, 5)).unwrap();
        e.submit_order(make_order(2, Side::Ask, 100.0, 5)).unwrap();
        let trades = e.submit_order(make_order(3, Side::Bid, 100.0, 8)).unwrap();
        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].maker_id, OrderId(1));
        assert_eq!(trades[1].maker_id, OrderId(2));
        assert_eq!(trades[0].qty, Qty(5));
        assert_eq!(trades[1].qty, Qty(3));
    }

    #[test]
    fn test_cancel_order() {
        let mut e = engine();
        e.submit_order(make_order(1, Side::Bid, 100.0, 10)).unwrap();
        let cancelled = e.cancel_order(OrderId(1)).unwrap();
        assert_eq!(cancelled.id, OrderId(1));
        assert!(e.book.best_bid().is_none());
    }

    #[test]
    fn test_l2_snapshot() {
        let mut e = engine();
        e.submit_order(make_order(1, Side::Bid, 99.0, 10)).unwrap();
        e.submit_order(make_order(2, Side::Bid, 100.0, 5)).unwrap();
        e.submit_order(make_order(3, Side::Ask, 101.0, 8)).unwrap();
        e.submit_order(make_order(4, Side::Ask, 102.0, 3)).unwrap();
        let snap = e.book.l2_snapshot(2);
        assert_eq!(snap.len(), 4);
    }

    #[test]
    fn test_market_order_consumes_liquidity() {
        let mut e = engine();
        e.submit_order(make_order(1, Side::Ask, 100.0, 10)).unwrap();
        let order = Order {
            id: OrderId(2),
            account: AccountId(1),
            side: Side::Bid,
            order_type: OrderType::Market,
            price: Price::from_f64(999.0),
            qty: Qty(5),
            remaining: Qty(5),
            tif: TimeInForce::GTC,
            timestamp: 0,
        };
        let trades = e.submit_order(order).unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].qty, Qty(5));
    }

    #[test]
    fn test_risk_rejects_high_notional() {
        let config = RiskConfig {
            max_notional: 1000 * PRICE_SCALE,
            ..Default::default()
        };
        let mut e = MatchingEngine::new(RiskEngine::new(config));
        let order = Order {
            id: OrderId(1),
            account: AccountId(1),
            side: Side::Bid,
            order_type: OrderType::Limit,
            price: Price::from_f64(100.0),
            qty: Qty(20),
            remaining: Qty(20),
            tif: TimeInForce::GTC,
            timestamp: 0,
        };
        assert!(e.submit_order(order).is_err());
    }

    #[test]
    fn test_ioc_partial_fill_no_rest() {
        let mut e = engine();
        e.submit_order(make_order(1, Side::Ask, 100.0, 5)).unwrap();
        let order = Order {
            id: OrderId(2),
            account: AccountId(1),
            side: Side::Bid,
            order_type: OrderType::IOC,
            price: Price::from_f64(100.0),
            qty: Qty(10),
            remaining: Qty(10),
            tif: TimeInForce::GTC,
            timestamp: 0,
        };
        let trades = e.submit_order(order).unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].qty, Qty(5));
        assert!(e.book.best_bid().is_none());
    }

    #[test]
    fn test_fok_full_fill_or_reject() {
        let mut e = engine();
        e.submit_order(make_order(1, Side::Ask, 100.0, 5)).unwrap();
        let order = Order {
            id: OrderId(2),
            account: AccountId(1),
            side: Side::Bid,
            order_type: OrderType::FOK,
            price: Price::from_f64(100.0),
            qty: Qty(10),
            remaining: Qty(10),
            tif: TimeInForce::GTC,
            timestamp: 0,
        };
        assert!(e.submit_order(order).is_err());
    }

    #[test]
    fn test_post_only_rejects_cross() {
        let mut e = engine();
        e.submit_order(make_order(1, Side::Ask, 100.0, 5)).unwrap();
        let order = Order {
            id: OrderId(2),
            account: AccountId(1),
            side: Side::Bid,
            order_type: OrderType::PostOnly,
            price: Price::from_f64(100.0),
            qty: Qty(10),
            remaining: Qty(10),
            tif: TimeInForce::GTC,
            timestamp: 0,
        };
        assert!(e.submit_order(order).is_err());
    }

    fn make_order_acct(id: u64, acct: u64, side: Side, price: f64, qty: u64) -> Order {
        Order {
            id: OrderId(id),
            account: AccountId(acct),
            side,
            order_type: OrderType::Limit,
            price: Price::from_f64(price),
            qty: Qty(qty),
            remaining: Qty(qty),
            tif: TimeInForce::GTC,
            timestamp: 0,
        }
    }

    #[test]
    fn test_stp_cancel_passive() {
        let mut e = MatchingEngine::with_stp(
            RiskEngine::new(RiskConfig::default()),
            StpPolicy::CancelPassive,
        );
        // Account 1 places a resting bid
        e.submit_order(make_order_acct(1, 1, Side::Bid, 100.0, 10))
            .unwrap();
        // Account 1 tries to cross with an ask — passive (bid) gets cancelled
        let trades = e
            .submit_order(make_order_acct(2, 1, Side::Ask, 100.0, 5))
            .unwrap();
        assert!(trades.is_empty());
        assert!(e.book.best_bid().is_none());
    }

    #[test]
    fn test_stp_cancel_aggressive() {
        let mut e = MatchingEngine::with_stp(
            RiskEngine::new(RiskConfig::default()),
            StpPolicy::CancelAggressive,
        );
        e.submit_order(make_order_acct(1, 1, Side::Bid, 100.0, 10))
            .unwrap();
        // Aggressive order (taker) gets cancelled, passive stays
        let trades = e
            .submit_order(make_order_acct(2, 1, Side::Ask, 100.0, 5))
            .unwrap();
        assert!(trades.is_empty());
        assert!(e.book.best_bid().is_some());
    }

    #[test]
    fn test_stp_decrement_and_cancel() {
        let mut e = MatchingEngine::with_stp(
            RiskEngine::new(RiskConfig::default()),
            StpPolicy::DecrementAndCancel,
        );
        e.submit_order(make_order_acct(1, 1, Side::Bid, 100.0, 10))
            .unwrap();
        let trades = e
            .submit_order(make_order_acct(2, 1, Side::Ask, 100.0, 10))
            .unwrap();
        // Should get partial fill (decrement), then both cancelled
        let total_filled: u64 = trades.iter().map(|t| t.qty.0).sum();
        assert!(total_filled > 0 && total_filled < 10);
    }

    #[test]
    fn test_l1_bbo() {
        let mut e = engine();
        e.submit_order(make_order(1, Side::Bid, 99.0, 10)).unwrap();
        e.submit_order(make_order(2, Side::Ask, 101.0, 8)).unwrap();
        let (bid, ask) = e.book.l1_bbo();
        assert_eq!(bid, Some((Price::from_f64(99.0), Qty(10))));
        assert_eq!(ask, Some((Price::from_f64(101.0), Qty(8))));
    }

    #[test]
    fn test_l2_depth() {
        let mut e = engine();
        e.submit_order(make_order(1, Side::Bid, 99.0, 10)).unwrap();
        e.submit_order(make_order(2, Side::Bid, 100.0, 5)).unwrap();
        e.submit_order(make_order(3, Side::Ask, 101.0, 8)).unwrap();
        let (bids, asks) = e.book.l2_depth(2);
        assert_eq!(bids.len(), 2);
        assert_eq!(asks.len(), 1);
    }

    #[test]
    fn test_l3_orders_at_price() {
        let mut e = engine();
        e.submit_order(make_order(1, Side::Bid, 100.0, 5)).unwrap();
        e.submit_order(make_order(2, Side::Bid, 100.0, 3)).unwrap();
        let orders = e.book.l3_orders_at(Price::from_f64(100.0), Side::Bid);
        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0].id, OrderId(1));
        assert_eq!(orders[1].id, OrderId(2));
    }

    #[test]
    fn test_multi_asset_registry() {
        let mut reg = MatchingRegistry::new();
        let sym_a = Symbol(1);
        let sym_b = Symbol(2);
        reg.register(sym_a, engine());
        reg.register(sym_b, engine());

        // Submit to sym_a
        reg.submit_order(sym_a, make_order(1, Side::Bid, 100.0, 10))
            .unwrap();
        // Submit to sym_b — independent book
        reg.submit_order(sym_b, make_order(2, Side::Ask, 200.0, 5))
            .unwrap();

        let ea = reg.get(sym_a).unwrap();
        assert_eq!(ea.book.best_bid(), Some(Price::from_f64(100.0)));
        let eb = reg.get(sym_b).unwrap();
        assert_eq!(eb.book.best_ask(), Some(Price::from_f64(200.0)));

        // Cancel from sym_a doesn't affect sym_b
        reg.cancel_order(sym_a, OrderId(1));
        assert!(reg.get(sym_a).unwrap().book.best_bid().is_none());
        assert!(reg.get(sym_b).unwrap().book.best_ask().is_some());
    }

    #[test]
    fn test_day_tif_rejects_after_session() {
        let mut e = MatchingEngine::with_session(
            RiskEngine::new(RiskConfig::default()),
            100,
        );
        // Session at tick 100, submit DAY order — should work (sequence 1 < 100)
        let order = Order {
            id: OrderId(1),
            account: AccountId(1),
            side: Side::Bid,
            order_type: OrderType::Limit,
            price: Price::from_f64(100.0),
            qty: Qty(10),
            remaining: Qty(10),
            tif: TimeInForce::DAY,
            timestamp: 0,
        };
        e.submit_order(order).unwrap();
        // Advance past session end
        e.sequence = 100;
        let order2 = Order {
            id: OrderId(2),
            account: AccountId(1),
            side: Side::Bid,
            order_type: OrderType::Limit,
            price: Price::from_f64(100.0),
            qty: Qty(10),
            remaining: Qty(10),
            tif: TimeInForce::DAY,
            timestamp: 0,
        };
        assert!(e.submit_order(order2).is_err());
    }

    #[test]
    fn test_day_tif_expire_day_orders() {
        let mut e = MatchingEngine::with_session(
            RiskEngine::new(RiskConfig::default()),
            50,
        );
        // Submit DAY order at tick 10
        let mut o1 = make_order(1, Side::Bid, 100.0, 10);
        o1.tif = TimeInForce::DAY;
        e.submit_order(o1).unwrap();
        // Submit GTC order at tick 20
        let mut o2 = make_order(2, Side::Bid, 99.0, 5);
        o2.tif = TimeInForce::GTC;
        e.submit_order(o2).unwrap();

        // Expire at session end — DAY order (tick < 50) removed, GTC stays
        e.session_end = 30;
        let expired = e.expire_day_orders();
        assert_eq!(expired, 1);
        assert!(e.book.best_bid() == Some(Price::from_f64(99.0)));
    }
}
