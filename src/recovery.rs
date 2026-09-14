use crate::engine::MatchingEngine;
use crate::risk::RiskEngine;
use crate::types::*;
use crate::wal::{self, EventType, WalReader};
use std::io;

pub struct Recovery;

impl Recovery {
    /// Replay WAL file to rebuild engine state.
    pub fn replay_wal(path: &str, risk: RiskEngine) -> io::Result<MatchingEngine> {
        let mut reader = WalReader::open(path)?;
        if !reader.read_header()? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid WAL header",
            ));
        }

        let mut engine = MatchingEngine::new(risk);

        while let Some(event) = reader.next_event()? {
            match event.event_type {
                t if t == EventType::INSERT => {
                    let order = Order {
                        id: OrderId(event.order_id),
                        account: AccountId(event.account_id),
                        side: match event.side {
                            0 => Side::Bid,
                            _ => Side::Ask,
                        },
                        order_type: match event.order_type {
                            0 => OrderType::Limit,
                            1 => OrderType::Market,
                            2 => OrderType::IOC,
                            3 => OrderType::FOK,
                            _ => OrderType::PostOnly,
                        },
                        price: Price(event.price),
                        qty: Qty(event.qty),
                        remaining: Qty(event.qty),
                        tif: TimeInForce::GTC,
                        timestamp: event.sequence_id,
                    };
                    let _ = engine.submit_order(order);
                }
                t if t == EventType::CANCEL => {
                    engine.cancel_order(OrderId(event.order_id));
                }
                t if t == EventType::SESSION_END => {
                    engine.session_end = event.sequence_id;
                    engine.expire_day_orders();
                }
                _ => {}
            }
        }

        Ok(engine)
    }

    /// Count events in a WAL file (valid + corrupted).
    pub fn count_events(path: &str) -> io::Result<(u64, u64)> {
        let mut reader = WalReader::open(path)?;
        if !reader.read_header()? {
            return Ok((0, 0));
        }
        let mut valid = 0u64;
        while let Some(event) = reader.next_event()? {
            let _ = event;
            valid += 1;
        }
        let file_size = std::fs::metadata(path)?.len();
        let total_slots = (file_size - 8) / wal::EVENT_SIZE as u64;
        let corrupted = total_slots - valid;
        Ok((valid, corrupted))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::{WalEvent, WalWriter};
    use crate::RiskConfig;
    use std::io::Write;
    use std::time::SystemTime;

    fn ts() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }

    #[test]
    fn test_replay_insert_cancel() {
        let path = "test_replay_insert_cancel.wal";
        {
            let mut writer = WalWriter::create(path).unwrap();
            writer
                .append(WalEvent::new_insert(1, ts(), 100, 1, 1, 10000, 10, 0, 0))
                .unwrap();
            writer
                .append(WalEvent::new_insert(2, ts(), 101, 1, 1, 20000, 5, 0, 0))
                .unwrap();
            writer
                .append(WalEvent::new_cancel(3, ts(), 100))
                .unwrap();
            writer.flush().unwrap();
        }

        let engine = Recovery::replay_wal(path, RiskEngine::new(RiskConfig::default())).unwrap();
        assert_eq!(engine.book.best_bid(), Some(Price(20000)));
        // Order 100 was cancelled, only 101 remains
        let orders = engine.book.l3_orders_at(Price(20000), Side::Bid);
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].id, OrderId(101));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_replay_corrupt_event() {
        let path = "test_replay_corrupt.wal";
        {
            let mut writer = WalWriter::create(path).unwrap();
            writer
                .append(WalEvent::new_insert(1, ts(), 100, 1, 1, 10000, 10, 0, 0))
                .unwrap();
            writer.flush().unwrap();
            // Write a corrupted event (bad CRC)
            let mut bad = [0u8; wal::EVENT_SIZE];
            bad[0..8].copy_from_slice(&2u64.to_le_bytes());
            std::fs::OpenOptions::new()
                .write(true)
                .append(true)
                .open(path)
                .unwrap()
                .write_all(&bad)
                .unwrap();
        }

        let engine = Recovery::replay_wal(path, RiskEngine::new(RiskConfig::default())).unwrap();
        // Only the first valid event should be replayed
        let orders = engine.book.l3_orders_at(Price(10000), Side::Bid);
        assert_eq!(orders.len(), 1);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_count_events() {
        let path = "test_count_events.wal";
        {
            let mut writer = WalWriter::create(path).unwrap();
            for i in 0..100u64 {
                writer
                    .append(WalEvent::new_insert(i, ts(), i, 1, 1, 10000, 10, 0, 0))
                    .unwrap();
            }
            writer.flush().unwrap();
        }

        let (valid, _) = Recovery::count_events(path).unwrap();
        assert_eq!(valid, 100);
        std::fs::remove_file(path).ok();
    }
}
