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
            outstanding_writes: HashSet::new(),
            outstanding_reads: HashSet::new(),
        }
    }

    pub(crate) fn min_read_pos(&self) -> &Cursor {
        // If there are no active receivers, return the min writer pos
        // (the queue appears initially empty for new receivers)
        if let Some(c)=self.outstanding_reads.iter().min() {
            c
        } else {
            dbg!(self.min_write_pos())
        }
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
    pub fn new(nbytes: usize) -> Self {
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

pub trait ChannelFactory {
    fn sender(&self)->Sender;
    fn receiver(&self)->Receiver;
}

impl ChannelFactory for Arc<Channel> {
    fn sender(&self)->Sender {
        Sender::new(self.clone())
    }

    fn receiver(&self)->Receiver {
        Receiver::new(self.clone())
    }
}

pub fn channel(nbytes: usize) -> (Sender, Receiver) {
    let ch = Arc::new(Channel::new(nbytes));
    (ch.sender(),ch.receiver())
}
