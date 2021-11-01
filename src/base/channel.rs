use super::{cursor::Cursor, receiver::Receiver, sender::Sender};

use std::{
    alloc::{self, Layout},
    collections::HashSet,
    ptr::NonNull,
    sync::Arc,
};

use parking_lot::{Condvar, RwLock};

pub(crate) struct Channel_ {
    pub(crate) ptr: NonNull<u8>,
    pub(crate) capacity: usize,
    pub(crate) is_accepting_writes: bool,
    pub(crate) high_mark: isize,
    pub(crate) space_available: Condvar,
    /// max of all writers
    pub(crate) head: Cursor,
    pub(crate) outstanding_writes: HashSet<Cursor>,
    pub(crate) outstanding_reads: HashSet<Cursor>,
}

pub struct Channel(pub(crate) Arc<RwLock<Channel_>>);

impl Drop for Channel_ {
    fn drop(&mut self) {
        if self.capacity > 0 {
            unsafe {
                let layout = Layout::from_size_align_unchecked(self.capacity, 1 << 12);
                alloc::dealloc(self.ptr.as_ptr(), layout)
            }
        }
    }
}

impl Channel {
    pub fn new(nbytes: usize) -> Self {
        // Align to 4096
        let layout = Layout::from_size_align(nbytes, 1 << 12).unwrap();
        let ptr = unsafe { alloc::alloc(layout) };
        let ptr = match NonNull::new(ptr) {
            Some(p) => p,
            None => alloc::handle_alloc_error(layout),
        };
        Channel(Arc::new(RwLock::new(Channel_ {
            ptr,
            capacity: nbytes,
            is_accepting_writes: true,
            high_mark: 0,
            space_available: Condvar::new(),
            head: Cursor::zero(),
            outstanding_writes: HashSet::new(),
            outstanding_reads: HashSet::new(),
        })))
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
