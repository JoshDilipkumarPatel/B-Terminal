use crossbeam::queue::ArrayQueue;
use std::sync::Arc;

/// A lock-free, zero-allocation ring buffer for high-frequency market data ingestion.
/// By utilizing `crossbeam::queue::ArrayQueue`, we avoid heap allocations (`Vec::push`) 
/// and OS-level lock contention (`Mutex`/`RwLock`), keeping the critical path bounded 
/// within nanoseconds.
pub struct LockFreeTickQueue<T> {
    queue: Arc<ArrayQueue<T>>,
}

impl<T> LockFreeTickQueue<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: Arc::new(ArrayQueue::new(capacity)),
        }
    }

    /// Enqueues an item without blocking or allocating memory.
    /// Returns an error if the queue is full (which triggers the circuit breaker 
    /// or drops the packet in chaos testing).
    pub fn push(&self, item: T) -> Result<(), T> {
        self.queue.push(item)
    }

    /// Dequeues an item without blocking.
    /// Returns `None` if empty.
    pub fn pop(&self) -> Option<T> {
        self.queue.pop()
    }

    pub fn capacity(&self) -> usize {
        self.queue.capacity()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

impl<T> Clone for LockFreeTickQueue<T> {
    fn clone(&self) -> Self {
        Self {
            queue: Arc::clone(&self.queue),
        }
    }
}
