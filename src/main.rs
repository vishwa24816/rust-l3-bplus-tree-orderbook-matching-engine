use matching_engine::*;

fn main() {
    let risk = RiskEngine::new(RiskConfig::default());
    let mut engine = MatchingEngine::new(risk);

    let order = Order {
        id: OrderId(1),
        account: AccountId(1),
        side: Side::Bid,
        order_type: OrderType::Limit,
        price: Price::from_f64(100.0),
        qty: Qty(10),
        remaining: Qty(10),
        tif: TimeInForce::GTC,
        timestamp: 0,
    };

    match engine.submit_order(order) {
        Ok(trades) => println!("Trades: {:?}", trades),
        Err(reason) => println!("Rejected: {:?}", reason),
    }
}
