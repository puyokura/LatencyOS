// ring_buffer.rs - Lock-Free Single-Producer Single-Consumer (SPSC) Ring Buffer
//
// Worst-case execution time: Documented per function.

use crate::gpu::FrameHandle;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

pub static CAPTURE_TO_ENCODE_RING: SpscRingBuffer<FrameHandle, 16> =
    SpscRingBuffer::new(FrameHandle::empty());

pub static ENCODE_TO_NET_RING: SpscRingBuffer<FrameHandle, 16> =
    SpscRingBuffer::new(FrameHandle::empty());

#[allow(dead_code)]
pub struct SpscRingBuffer<T: Copy, const CAP: usize> {
    buffer: [UnsafeCell<T>; CAP],
    head: AtomicUsize,
    tail: AtomicUsize,
}

unsafe impl<T: Copy + Send, const CAP: usize> Sync for SpscRingBuffer<T, CAP> {}
unsafe impl<T: Copy + Send, const CAP: usize> Send for SpscRingBuffer<T, CAP> {}

#[allow(dead_code)]
impl<T: Copy, const CAP: usize> SpscRingBuffer<T, CAP> {
    // Function: new
    // Description: Initialize a new fixed-capacity SPSC ring buffer.
    // Worst-case execution time: ~10 ns
    pub const fn new(default_val: T) -> Self {
        // Const initialization of UnsafeCell array
        const fn make_cells<T: Copy, const N: usize>(val: T) -> [UnsafeCell<T>; N] {
            unsafe {
                let mut arr: [UnsafeCell<T>; N] = core::mem::MaybeUninit::uninit().assume_init();
                let mut i = 0;
                while i < N {
                    arr[i] = UnsafeCell::new(val);
                    i += 1;
                }
                arr
            }
        }

        Self {
            buffer: make_cells(default_val),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    // Function: push
    // Description: Push an item to the ring buffer (called by Producer core only).
    // Worst-case execution time: ~15 ns
    pub fn push(&self, item: T) -> Result<(), ()> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        if head.wrapping_sub(tail) >= CAP {
            return Err(()); // Buffer full
        }

        let idx = head % CAP;
        unsafe {
            *self.buffer[idx].get() = item;
        }
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    // Function: pop
    // Description: Pop an item from the ring buffer (called by Consumer core only).
    // Worst-case execution time: ~15 ns
    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);

        if tail == head {
            return None; // Buffer empty
        }

        let idx = tail % CAP;
        let item = unsafe { *self.buffer[idx].get() };
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Some(item)
    }

    // Function: is_empty
    // Description: Check if buffer is empty.
    // Worst-case execution time: ~8 ns
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tail.load(Ordering::Relaxed) == self.head.load(Ordering::Acquire)
    }

    // Function: is_full
    // Description: Check if buffer is full.
    // Worst-case execution time: ~8 ns
    #[inline]
    pub fn is_full(&self) -> bool {
        self.head.load(Ordering::Relaxed).wrapping_sub(self.tail.load(Ordering::Acquire)) >= CAP
    }

    // Function: len
    // Description: Return current number of elements in buffer.
    // Worst-case execution time: ~8 ns
    #[inline]
    pub fn len(&self) -> usize {
        self.head.load(Ordering::Relaxed).wrapping_sub(self.tail.load(Ordering::Acquire))
    }
}
