use std::sync::atomic::{AtomicUsize, Ordering};
use std::cell::UnsafeCell;

/// Simulates a FIX protocol or internal normalized market data message.
/// In the real FPGA implementation, this would map perfectly to a C struct layout.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FpgaMessage {
    pub timestamp_ns: u64,
    pub symbol_id: u32,
    pub is_bid: bool,
    pub price: f64,
    pub quantity: f64,
}

impl Default for FpgaMessage {
    fn default() -> Self {
        Self {
            timestamp_ns: 0,
            symbol_id: 0,
            is_bid: false,
            price: 0.0,
            quantity: 0.0,
        }
    }
}

/// A prototype lock-free, zero-allocation ring buffer simulating a DMA shared memory queue
/// between the Host CPU and an FPGA network accelerator.
/// We use `unsafe` Rust to mimic the raw memory manipulation that occurs when
/// reading hardware registers directly.
pub struct ZeroCopyRingBuffer {
    buffer: Vec<UnsafeCell<FpgaMessage>>,
    head: AtomicUsize,
    tail: AtomicUsize,
    capacity: usize,
}

// SAFETY: ZeroCopyRingBuffer implements a single-producer, single-consumer (SPSC)
// lock-free queue. The safety of Send + Sync relies on these invariants:
// 1. Only ONE thread calls dma_write() (the producer/FPGA simulator).
// 2. Only ONE thread calls cpu_read() (the consumer/host CPU).
// 3. The producer exclusively owns `tail`; the consumer exclusively owns `head`.
// 4. Release/Acquire ordering on stores/loads of the *other* thread's index
//    establishes a happens-before relationship with the UnsafeCell data access.
// Violating invariants 1 or 2 (e.g., calling dma_write from two threads) is UB.
unsafe impl Send for ZeroCopyRingBuffer {}
unsafe impl Sync for ZeroCopyRingBuffer {}

impl ZeroCopyRingBuffer {
    pub fn new(capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffer.push(UnsafeCell::new(FpgaMessage::default()));
        }
        Self {
            buffer,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            capacity,
        }
    }

    /// Simulates the FPGA writing data directly into the host's physical memory via DMA.
    pub fn dma_write(&self, msg: FpgaMessage) -> Result<(), &'static str> {
        let current_tail = self.tail.load(Ordering::Relaxed);
        let next_tail = (current_tail + 1) % self.capacity;

        if next_tail == self.head.load(Ordering::Acquire) {
            return Err("ZeroCopy queue full (DMA overrun)");
        }

        // UNSAFE: We are bypassing Rust's borrow checker to write directly to the buffer,
        // simulating hardware injecting bits into physical memory.
        unsafe {
            *self.buffer[current_tail].get() = msg;
        }

        self.tail.store(next_tail, Ordering::Release);
        Ok(())
    }

    /// Simulates the Host CPU reading the data directly from physical memory without any 
    /// intermediate kernel copies or socket polling.
    pub fn cpu_read(&self) -> Option<FpgaMessage> {
        let current_head = self.head.load(Ordering::Relaxed);

        if current_head == self.tail.load(Ordering::Acquire) {
            return None; // Empty
        }

        // UNSAFE: Read directly from the simulated DMA region.
        let msg = unsafe {
            *self.buffer[current_head].get()
        };

        self.head.store((current_head + 1) % self.capacity, Ordering::Release);
        Some(msg)
    }
}
