use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU64, Ordering};

pub const CACHE_LINE: usize = 64;

#[repr(align(64))]
struct CacheAligned<T>(T);

pub struct SpscRing<T> {
    // ponytail: Cache-line padded head/tail to prevent false sharing
    // Upgrade: if profiling shows contention on multi-core, add per-thread sharding
    head: CacheAligned<AtomicU64>,
    tail: CacheAligned<AtomicU64>,
    buf: UnsafeCell<Vec<T>>,
}

// Safety: SPSC — only one thread writes head, one thread writes tail
unsafe impl<T: Send> Send for SpscRing<T> {}
unsafe impl<T: Send> Sync for SpscRing<T> {}

impl<T: Clone + Default> SpscRing<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            head: CacheAligned(AtomicU64::new(0)),
            tail: CacheAligned(AtomicU64::new(0)),
            buf: UnsafeCell::new(vec![T::default(); capacity]),
        }
    }

    pub fn capacity(&self) -> usize {
        // Safety: buf is only accessed via UnsafeCell, which is fine for SPSC
        unsafe { (*self.buf.get()).len() }
    }

    /// Producer: try to push. Returns false if full.
    #[inline]
    pub fn push(&self, item: T) -> Result<(), T> {
        let head = self.head.0.load(Ordering::Relaxed);
        let tail = self.tail.0.load(Ordering::Acquire);
        let cap = self.capacity() as u64;

        if head - tail >= cap {
            return Err(item);
        }

        let idx = (head % cap) as usize;
        unsafe {
            (&mut *self.buf.get())[idx] = item;
        }
        self.head.0.store(head + 1, Ordering::Release);
        Ok(())
    }

    /// Consumer: try to pop. Returns None if empty.
    #[inline]
    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.0.load(Ordering::Relaxed);
        let head = self.head.0.load(Ordering::Acquire);

        if tail >= head {
            return None;
        }

        let cap = self.capacity() as u64;
        let idx = (tail % cap) as usize;
        let item = unsafe { (&*self.buf.get())[idx].clone() };
        self.tail.0.store(tail + 1, Ordering::Release);
        Some(item)
    }

    pub fn is_empty(&self) -> bool {
        let tail = self.tail.0.load(Ordering::Relaxed);
        let head = self.head.0.load(Ordering::Acquire);
        tail >= head
    }

    pub fn len(&self) -> u64 {
        let head = self.head.0.load(Ordering::Acquire);
        let tail = self.tail.0.load(Ordering::Relaxed);
        head.saturating_sub(tail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop_roundtrip() {
        let ring = SpscRing::new(8);
        for i in 0..8u64 {
            ring.push(i).unwrap();
        }
        assert!(ring.push(99).is_err()); // full
        for i in 0..8u64 {
            assert_eq!(ring.pop(), Some(i));
        }
        assert_eq!(ring.pop(), None); // empty
    }

    #[test]
    fn test_spsc_concurrent() {
        use std::sync::Arc;
        use std::thread;

        let ring = Arc::new(SpscRing::new(1024));
        let ring2 = ring.clone();

        let producer = thread::spawn(move || {
            for i in 0..1000u64 {
                while ring.push(i).is_err() {
                    std::hint::spin_loop();
                }
            }
        });

        let mut received = Vec::new();
        loop {
            if let Some(v) = ring2.pop() {
                received.push(v);
                if received.len() == 1000 {
                    break;
                }
            } else {
                std::hint::spin_loop();
            }
        }

        producer.join().unwrap();
        assert_eq!(received.len(), 1000);
        // Verify ordering
        for i in 0..1000u64 {
            assert_eq!(received[i as usize], i);
        }
    }
}
