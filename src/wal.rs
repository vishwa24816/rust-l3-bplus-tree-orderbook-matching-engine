use memmap2::MmapMut;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};

pub const EVENT_SIZE: usize = 80;
pub const WAL_MAGIC: u32 = 0x5741_4C31; // "WAL1"
const CHUNK_SIZE: usize = 1024 * 1024; // ponytail: 1MB chunks, extend on demand

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WalEvent {
    pub sequence_id: u64,
    pub timestamp_ns: u64,
    pub event_type: u32,
    pub order_id: u64,
    pub account_id: u64,
    pub symbol_id: u32,
    pub price: i64,
    pub qty: u64,
    pub side: u32,
    pub order_type: u32,
    pub crc32: u32,
}

pub struct EventType;

impl EventType {
    pub const INSERT: u32 = 0;
    pub const CANCEL: u32 = 1;
    pub const REPLACE: u32 = 2;
    pub const SESSION_END: u32 = 3;
}

impl WalEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new_insert(
        seq: u64,
        ts: u64,
        order_id: u64,
        account_id: u64,
        symbol_id: u32,
        price: i64,
        qty: u64,
        side: u32,
        order_type: u32,
    ) -> Self {
        let mut e = Self {
            sequence_id: seq,
            timestamp_ns: ts,
            event_type: EventType::INSERT,
            order_id,
            account_id,
            symbol_id,
            price,
            qty,
            side,
            order_type,
            crc32: 0,
        };
        e.crc32 = e.compute_crc32();
        e
    }

    pub fn new_cancel(seq: u64, ts: u64, order_id: u64) -> Self {
        let mut e = Self {
            sequence_id: seq,
            timestamp_ns: ts,
            event_type: EventType::CANCEL,
            order_id,
            account_id: 0,
            symbol_id: 0,
            price: 0,
            qty: 0,
            side: 0,
            order_type: 0,
            crc32: 0,
        };
        e.crc32 = e.compute_crc32();
        e
    }

    pub fn new_session_end(seq: u64, ts: u64) -> Self {
        let mut e = Self {
            sequence_id: seq,
            timestamp_ns: ts,
            event_type: EventType::SESSION_END,
            order_id: 0,
            account_id: 0,
            symbol_id: 0,
            price: 0,
            qty: 0,
            side: 0,
            order_type: 0,
            crc32: 0,
        };
        e.crc32 = e.compute_crc32();
        e
    }

    pub fn to_bytes(self) -> [u8; EVENT_SIZE] {
        let mut buf = [0u8; EVENT_SIZE];
        unsafe {
            std::ptr::copy_nonoverlapping(
                &self as *const Self as *const u8,
                buf.as_mut_ptr(),
                EVENT_SIZE,
            );
        }
        buf
    }

    /// Write this event directly into a memory-mapped slice at the given offset.
    /// Zero-copy: just a pointer copy into the mmap region.
    #[inline]
    pub fn write_to_mmap(self, dst: &mut [u8]) {
        unsafe {
            std::ptr::copy_nonoverlapping(
                &self as *const Self as *const u8,
                dst.as_mut_ptr(),
                EVENT_SIZE,
            );
        }
    }

    pub fn from_bytes(buf: &[u8; EVENT_SIZE]) -> Option<Self> {
        let mut event: Self = unsafe { std::ptr::read(buf.as_ptr() as *const Self) };
        let stored_crc = event.crc32;
        event.crc32 = 0;
        if stored_crc != event.compute_crc32() {
            return None;
        }
        event.crc32 = stored_crc;
        Some(event)
    }

    /// Read an event directly from a memory-mapped slice at the given offset.
    #[inline]
    pub fn read_from_mmap(src: &[u8]) -> Option<Self> {
        if src.len() < EVENT_SIZE {
            return None;
        }
        let mut event: Self = unsafe { std::ptr::read(src.as_ptr() as *const Self) };
        let stored_crc = event.crc32;
        event.crc32 = 0;
        if stored_crc != event.compute_crc32() {
            return None;
        }
        event.crc32 = stored_crc;
        Some(event)
    }

    fn compute_crc32(&self) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        let bytes = unsafe {
            std::slice::from_raw_parts(self as *const Self as *const u8, EVENT_SIZE - 4)
        };
        for &b in bytes {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xEDB8_8320 & (-(crc as i32) as u32));
            }
        }
        !crc
    }
}

pub struct WalWriter {
    file: File,
    mmap: MmapMut,
    offset: u64,
    capacity: u64,
}

impl WalWriter {
    pub fn create(path: &str) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .truncate(true)
            .open(path)?;

        let cap = CHUNK_SIZE as u64;
        file.set_len(cap)?;

        let mut mmap = unsafe { MmapMut::map_mut(&file)? };

        // Write header
        let header: [u8; 8] = [
            WAL_MAGIC.to_le_bytes()[0],
            WAL_MAGIC.to_le_bytes()[1],
            WAL_MAGIC.to_le_bytes()[2],
            WAL_MAGIC.to_le_bytes()[3],
            1u32.to_le_bytes()[0],
            1u32.to_le_bytes()[1],
            1u32.to_le_bytes()[2],
            1u32.to_le_bytes()[3],
        ];
        mmap[..8].copy_from_slice(&header);
        mmap.flush_range(0, 8)?;

        Ok(Self {
            file,
            mmap,
            offset: 8,
            capacity: cap,
        })
    }

    pub fn open_append(path: &str) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;

        let file_len = file.metadata()?.len();
        let cap = file_len.max(CHUNK_SIZE as u64);
        file.set_len(cap)?;

        let mmap = unsafe { MmapMut::map_mut(&file)? };

        Ok(Self {
            file,
            mmap,
            offset: file_len,
            capacity: cap,
        })
    }

    /// Append a single event. Zero-copy write directly into mmap.
    #[inline]
    pub fn append(&mut self, event: WalEvent) -> io::Result<u64> {
        let pos = self.offset;
        let start = self.offset as usize;

        if start + EVENT_SIZE > self.capacity as usize {
            self.extend()?;
        }

        event.write_to_mmap(&mut self.mmap[start..start + EVENT_SIZE]);
        self.offset += EVENT_SIZE as u64;
        Ok(pos)
    }

    /// Append a batch of events. Single flush for the entire batch.
    #[inline]
    pub fn append_batch(&mut self, events: &[WalEvent]) -> io::Result<u64> {
        let start_offset = self.offset;
        let batch_bytes = events.len() * EVENT_SIZE;
        let start = self.offset as usize;

        if start + batch_bytes > self.capacity as usize {
            self.extend()?;
        }

        for (i, event) in events.iter().enumerate() {
            let off = start + i * EVENT_SIZE;
            event.write_to_mmap(&mut self.mmap[off..off + EVENT_SIZE]);
        }
        self.offset += batch_bytes as u64;
        Ok(start_offset)
    }

    /// Flush a range of the mmap to disk.
    #[inline]
    pub fn flush_range(&self, offset: u64, len: usize) -> io::Result<()> {
        self.mmap.flush_range(offset as usize, len)
    }

    /// Flush the entire mmap to disk.
    pub fn flush(&self) -> io::Result<()> {
        self.mmap.flush()
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    fn extend(&mut self) -> io::Result<()> {
        // ponytail: double capacity on extend, cap at 256MB
        let new_cap = (self.capacity * 2).min(256 * 1024 * 1024);
        self.file.set_len(new_cap)?;
        self.mmap = unsafe { MmapMut::map_mut(&self.file)? };
        self.capacity = new_cap;
        Ok(())
    }
}

pub struct WalReader {
    file: File,
    pos: u64,
    end: u64,
}

impl WalReader {
    pub fn open(path: &str) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).open(path)?;
        let end = file.metadata()?.len();
        Ok(Self { file, pos: 0, end })
    }

    pub fn read_header(&mut self) -> io::Result<bool> {
        let mut header = [0u8; 8];
        self.file.read_exact(&mut header)?;
        let magic = u32::from_le_bytes(header[..4].try_into().unwrap());
        if magic != WAL_MAGIC {
            return Ok(false);
        }
        self.pos = 8;
        Ok(true)
    }

    pub fn next_event(&mut self) -> io::Result<Option<WalEvent>> {
        if self.pos + EVENT_SIZE as u64 > self.end {
            return Ok(None);
        }
        let mut buf = [0u8; EVENT_SIZE];
        self.file.read_exact(&mut buf)?;
        self.pos += EVENT_SIZE as u64;
        match WalEvent::from_bytes(&buf) {
            Some(e) => Ok(Some(e)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_roundtrip() {
        let path = "test_wal_roundtrip.wal";
        {
            let mut writer = WalWriter::create(path).unwrap();
            let event = WalEvent::new_insert(1, 12345, 100, 1, 1, 10000, 10, 0, 0);
            writer.append(event).unwrap();
            writer.flush().unwrap();
        }

        let mut reader = WalReader::open(path).unwrap();
        assert!(reader.read_header().unwrap());
        let event = reader.next_event().unwrap().unwrap();
        assert_eq!(event.sequence_id, 1);
        assert_eq!(event.order_id, 100);
        assert_eq!(event.price, 10000);
        assert_eq!(event.qty, 10);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_wal_batch_append() {
        let path = "test_wal_batch.wal";
        {
            let mut writer = WalWriter::create(path).unwrap();
            let events: Vec<WalEvent> = (0..100)
                .map(|i| WalEvent::new_insert(i, i * 1000, i + 100, 1, 1, 10000, 10, 0, 0))
                .collect();
            writer.append_batch(&events).unwrap();
            writer.flush().unwrap();
        }

        let mut reader = WalReader::open(path).unwrap();
        assert!(reader.read_header().unwrap());
        let mut count = 0;
        while let Ok(Some(event)) = reader.next_event() {
            assert_eq!(event.sequence_id, count);
            count += 1;
        }
        assert_eq!(count, 100);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_wal_extend() {
        let path = "test_wal_extend.wal";
        {
            let mut writer = WalWriter::create(path).unwrap();
            // Fill beyond initial 1MB chunk (1MB / 80 bytes = 13107 events)
            for i in 0..14000u64 {
                writer
                    .append(WalEvent::new_insert(i, i, i + 200, 1, 1, 10000, 10, 0, 0))
                    .unwrap();
            }
            writer.flush().unwrap();
        }

        let mut reader = WalReader::open(path).unwrap();
        assert!(reader.read_header().unwrap());
        let mut count = 0u64;
        while let Ok(Some(event)) = reader.next_event() {
            assert_eq!(event.sequence_id, count);
            count += 1;
        }
        assert_eq!(count, 14000);
        std::fs::remove_file(path).ok();
    }
}
