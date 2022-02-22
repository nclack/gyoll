use std::{
    alloc::{self, Layout},
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
    ptr::NonNull,
    sync::Arc,
};

use parking_lot::{Condvar, Mutex, RwLock};

use super::{cursor::Cursor, receiver::Receiver, sender::Sender};

// FIXME: Counter gets leaked to receiver and is kind of gross. Isn't there
//        something better?
#[derive(Debug)]
pub(crate) struct Counter<T> {
    inner: HashMap<T, usize>,
}

impl<T> Counter<T>
where
    T: Eq + Hash + Ord + Copy + Debug,
{
    fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
    pub(crate) fn insert(&mut self, v: T) {
        *self.inner.entry(v).or_insert(0) += 1;
    }
    pub(crate) fn remove(&mut self, v: &T) {
        if let Entry::Occupied(mut o) = self.inner.entry(*v) {
            if *o.get() <= 1 {
                o.remove_entry();
            } else {
                *o.get_mut() -= 1;
            }
        }
    }
    fn min(&self) -> Option<&T> {
        self.inner.keys().min()
    }
}

pub(crate) struct RawChannel {
    pub(crate) ptr: NonNull<u8>,
    pub(crate) capacity: usize,

    /// when closing the channel, we stop accepting writes
    pub(crate) is_accepting_writes: bool,
    pub(crate) high_mark: isize,

    /// max of all writers
    pub(crate) head: Cursor,

    pub(crate) outstanding_writes: HashSet<Cursor>,
    pub(crate) outstanding_reads: Counter<Cursor>,
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
            outstanding_reads: Counter::new(),
        }
    }

    pub(crate) fn min_read_pos(&self) -> &Cursor {
        // If there are no active receivers, return the min writer pos
        // (the queue appears initially empty for new receivers)
        if let Some(c) = self.outstanding_reads.min() {
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
mod test {
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
