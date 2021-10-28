#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(dead_code)]

use core::marker::{Send, Sync};
use std::{
    collections::binary_heap::Iter,
    mem::size_of_val,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    sync::Arc,
};

use parking_lot::{Condvar, RwLock};

use std::alloc::{self, Layout};

struct Chan {
    ptr: NonNull<u8>,
    capacity: usize,
    is_accepting_writes: bool,
    space_available: Condvar,
    head: NonNull<u8>,
}

pub struct Channel(Arc<RwLock<Chan>>);

pub struct Sender {
    channel: Arc<RwLock<Chan>>,
}

pub struct Receiver {}

pub struct MutRegion<'a> {
    buf: &'a mut [u8],
}

pub struct Available<'a> {
    rx: &'a Receiver,
}

impl Drop for Chan {
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
        Channel(Arc::new(RwLock::new(Chan {
            ptr,
            capacity: nbytes,
            is_accepting_writes: true,
            space_available: Condvar::new(),
            head: 0,
        })))
    }

    pub fn sender(&self) -> Sender {
        Sender {
            channel: self.0.clone(),
        }
    }

    pub fn receiver(&self) -> Receiver {
        todo!()
    }
}

impl Sender {
    /// Reserves a mutable region of the channel.
    ///
    /// Blocks until a region is available.
    /// TODO: Consider a future so it's clear this can block.
    ///
    /// Returns None when the channel is unwritable or `nbytes` exceeds the
    /// channels `capacity`.
    pub fn map(&mut self, nbytes: usize) -> Option<MutRegion> {
        let padded_bytes = nbytes + std::mem::size_of_val(&nbytes);
        if padded_bytes > self.channel.read().capacity {
            None
        } else {
            // TODO: Later this will require a write lock while we modify the head etc.
            let c = self.channel.write();
            let buf = unsafe {
                let ptr = c.head.as_ptr();
                *(ptr as *mut usize) = padded_bytes;
                std::slice::from_raw_parts_mut(ptr.offset(size_of_val(&nbytes) as _), nbytes)
            };
            Some(MutRegion { buf })
        }
    }
}

unsafe impl Send for Sender {}
unsafe impl Sync for Sender {}

impl<'a> AsMut<[u8]> for MutRegion<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.buf
    }
}

impl<'a> Deref for MutRegion<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        todo!()
    }
}

impl<'a> DerefMut for MutRegion<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        todo!()
    }
}

impl<'a> Drop for MutRegion<'a> {
    fn drop(&mut self) {
        todo!()
    }
}

impl Receiver {
    pub fn acquire(&self) -> Option<Available> {
        todo!()
    }
}

impl<'a> Iterator for Available<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}
