use std::{
    alloc::{self, Layout},
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt::{Debug, Display},
    hash::Hash,
    ptr::NonNull,
    sync::Arc,
};

use parking_lot::{Condvar, Mutex, RwLock};

use super::{
    counter::Counter,
    cursor::{BegCursor, EndCursor, Interval},
    receiver::Receiver,
    sender::Sender,
};

#[derive(Debug)]
pub(crate) struct RawChannel {
    pub(crate) ptr: NonNull<u8>,
    pub(crate) capacity: usize,

    /// when closing the channel, we stop accepting writes
    pub(crate) is_accepting_writes: bool,

    // This is an offset that behaves like an EndCursor.
    // It is relevant for the read side.
    pub(crate) tmp_high_mark: Option<isize>,
    pub(crate) read_high_mark: Option<isize>,

    /// End of regions claimed for write
    pub(crate) write_tail: BegCursor,

    /// End of claimed writes.
    pub(crate) write_head: EndCursor,

    /// First unread byte. New receivers start here.
    pub(crate) read_tail: BegCursor,

    /// End of the readable bytes
    pub(crate) read_head: EndCursor,

    pub(crate) outstanding_writes: HashSet<Interval>,
    pub(crate) outstanding_reads: Counter<BegCursor>,
}
// FIXME: Counter gets leaked to receiver and is kind of gross. Isn't there
//        something better?

impl Display for RawChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "write:( {write} ) read:( {read} ) outstanding writes: {outw:?} reads:{outr:?}",
            write = Interval {
                beg: self.write_tail,
                end: self.write_head,
                high_mark: self.tmp_high_mark
            },
            read = Interval {
                beg: self.read_tail,
                end: self.read_head,
                high_mark: self.read_high_mark
            },
            outw = self.outstanding_writes,
            outr = self.outstanding_reads
        )
    }
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
            tmp_high_mark: None,
            read_high_mark: None,
            write_head: EndCursor::zero(),
            write_tail: BegCursor::zero(),
            read_head: EndCursor::zero(),
            read_tail: BegCursor::zero(),
            outstanding_writes: HashSet::new(),
            outstanding_reads: Counter::new(),
        }
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
        let mut ch = self.inner.lock();
        ch.is_accepting_writes = false;
        self.space_available.notify_all();
    }

    // Base pointer for the region controlled by the channel.
    // Used for debugging. Might not be desirable otherwise.
    pub fn as_ptr(&self) -> *const u8 {
        self.inner.lock().ptr.as_ptr()
    }
}

pub trait ChannelFactory {
    fn sender(&self) -> Sender;
    fn receiver(&self) -> Receiver;
}

impl ChannelFactory for Arc<Channel> {
    fn sender(&self) -> Sender {
        Sender::new(self.clone())
    }

    fn receiver(&self) -> Receiver {
        Receiver::new(self.clone())
    }
}

pub fn channel(nbytes: usize) -> (Sender, Receiver) {
    let ch = Arc::new(Channel::new(nbytes));
    (ch.sender(), ch.receiver())
}

#[cfg(test)]
mod tests {
    use crate::base::channel::Counter;

    #[test]
    fn counter_insert() {
        let mut c = Counter::new();
        c.insert(5);
        assert_eq!(c.min(), Some(&5));
    }

    #[test]
    fn counter_empty_min() {
        let c: Counter<i32> = Counter::new();
        assert_eq!(c.min(), None);
    }

    #[test]
    fn counter_remove() {
        let mut c = Counter::new();
        c.remove(&5);
        c.insert(5);
        c.insert(6);
        c.remove(&5);
        assert_eq!(c.min(), Some(&6));
    }
}
