use std::{
    alloc::{self, Layout},
    collections::HashSet,
    ptr::NonNull,
    sync::Arc,
};

use parking_lot::{Condvar, RwLock, Mutex};

use super::{cursor::Cursor, receiver::Receiver, sender::Sender};

pub(crate) struct RawChannel {
    pub(crate) ptr: NonNull<u8>,
    pub(crate) capacity: usize,

    /// when closing the channel, we stop accepting writes
    pub(crate) is_accepting_writes: bool,
    pub(crate) high_mark: isize,

    /// max of all writers
    pub(crate) head: Cursor,

    /// max of all readers
    pub(crate) tail: Cursor,

    pub(crate) outstanding_writes: HashSet<Cursor>,
    pub(crate) outstanding_reads: HashSet<Cursor>,
}

impl RawChannel {
    fn new(nbytes: usize) -> Self {
        // Align to 4096
        let layout = Layout::from_size_align(nbytes, 1 << 12).unwrap();
        let ptr = unsafe { std::alloc::alloc(layout) };
        let ptr = match NonNull::new(ptr) {
            Some(p) => p,
            None => std::alloc::handle_alloc_error(layout),
        };

        Self {
            ptr,
            capacity: nbytes,
            is_accepting_writes: true,
            high_mark: 0,
            head: Cursor::zero(),
            tail: Cursor::zero(),
            outstanding_writes: HashSet::new(),
            outstanding_reads: HashSet::new(),
        }
    }

    pub(crate) fn min_read_pos(&self) -> &Cursor {
        self.outstanding_reads.iter().min().unwrap_or(&self.tail)
    }

    pub(crate) fn min_write_pos(&self) -> &Cursor {
        self.outstanding_writes.iter().min().unwrap_or(&self.head)
    }
}

impl Drop for RawChannel {
    fn drop(&mut self) {
        if self.capacity > 0 {
            unsafe {
                let layout = Layout::from_size_align_unchecked(self.capacity, 1 << 12);
                alloc::dealloc(self.ptr.as_ptr(), layout)
            }
        }
    }
}

pub struct Channel {
    pub(crate) inner: Mutex<RawChannel>,
    pub(crate) space_available: Condvar,
}

impl Channel {
    pub(crate) fn new(nbytes: usize) -> Self {
        Channel {
            inner: Mutex::new(RawChannel::new(nbytes)),
            space_available: Condvar::new(),
        }
    }

    pub fn close(&self) {
        let mut ch=self.inner.lock();
        ch.is_accepting_writes = false;
        self.space_available.notify_all();
    }
}

pub fn channel(nbytes: usize) -> (Sender, Receiver) {
    let ch = Arc::new(Channel::new(nbytes));
    (Sender::new(ch.clone()), Receiver::new(ch.clone()))
}
