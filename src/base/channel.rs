use std::{alloc::{self,Layout}, collections::HashSet, ptr::NonNull, sync::Arc};

use parking_lot::{RwLock,Condvar};

use super::{cursor::Cursor, receiver::Receiver, sender::Sender, util::CondvarAny};

use crate::base::util;

pub(crate) struct RawChannel {
    pub(crate) ptr: NonNull<u8>,
    pub(crate) capacity: usize,

    /// when closing the channel, we stop accepting writes
    pub(crate) is_accepting_writes: bool,
    pub(crate) high_mark: isize,
    pub(crate) space_available: util::CondvarAny,

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
            space_available: CondvarAny::new(),
            head: Cursor::zero(),
            outstanding_writes: HashSet::new(),
            outstanding_reads: HashSet::new(),
        }
    }

    pub(crate) fn min_read_pos(&self) -> Option<&Cursor> {
        self.outstanding_reads.iter().min()
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

pub struct Channel(pub(crate) Arc<RwLock<RawChannel>>);

impl Channel {
    pub fn new(nbytes: usize) -> Self {
        Channel(Arc::new(RwLock::new(RawChannel::new(nbytes))))
    }

    pub fn stop(&self) {
        self.0.write().is_accepting_writes = false;
    }

    pub fn sender(&self) -> Sender {
        Sender::new(self)
    }

    pub fn receiver(&self) -> Receiver {
        Receiver::new(self)
    }
}
