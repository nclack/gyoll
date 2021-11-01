use std::{mem::size_of_val, sync::Arc};

use parking_lot::RwLock;

use super::{
    channel::{Channel, Channel_},
    cursor::Cursor,
    region::MutRegion,
};

pub struct Sender {
    channel: Arc<RwLock<Channel_>>,
}

unsafe impl Send for Sender {}
unsafe impl Sync for Sender {}

impl Sender {
    pub(crate) fn new(channel: &Channel) -> Self {
        Sender {
            channel: channel.0.clone(),
        }
    }

    /// Reserves a mutable region of the channel.
    ///
    /// Blocks until a region is available.
    ///
    /// Returns None when the channel is unwritable or `nbytes` exceeds the
    /// channels `capacity`.
    pub fn map(&mut self, nbytes: usize) -> Option<MutRegion> {
        if nbytes > self.channel.read().capacity {
            None
        } else {
            // 1. Get the address of the region
            // 2. Move the head
            let (cur, ptr) = {
                let mut c = self.channel.write();

                if !c.is_accepting_writes {
                    return None; // See Note A.
                }

                // TODO: block when space isn't available

                let cur = c.head.offset;
                let (beg, end) = c.head.inc_region(nbytes, c.capacity);

                while end > *c.outstanding_reads.iter().min().unwrap_or(&end) {
                    c.space_available.wait(c);
                }

                c.outstanding_writes.insert(beg.clone());
                c.head = end;

                if beg.offset == 0 {
                    c.high_mark = cur;
                }

                // See Note B.
                let ptr = unsafe { c.ptr.as_ptr().offset(beg.offset) };
                (beg, ptr)
            };

            // 3. Construct the region
            let buf = unsafe {
                *(ptr as *mut usize) = nbytes;
                std::slice::from_raw_parts_mut(ptr.offset(size_of_val(&nbytes) as _), nbytes)
            };
            Some(MutRegion {
                owner: self,
                cur,
                buf,
            })
        }
    }

    pub(crate) fn release(&self, beg: &Cursor) {
        let mut c = self.channel.write();
        c.outstanding_writes.remove(beg);
    }
}

#[cfg(test)]
mod test {
    use crate::base::{cursor::Cursor, region::MutRegion, Channel};

    #[test]
    #[rustfmt::skip]
    fn send_wrap_behavior() {
        let c = Channel::new(13);
        let mut tx = c.sender();
        {
            let reg = tx.map(5).unwrap();
            assert_eq!(reg.cur,Cursor { cycle: 0, offset: 0 });
        }
        {
            let reg = tx.map(5).unwrap();
            assert_eq!(reg.cur,Cursor { cycle: 0, offset: 5 });
        }
        {
            let reg = tx.map(5).unwrap();
            assert_eq!(reg.cur,Cursor { cycle: 1, offset: 0 });
            let c=c.0.read();
            assert_eq!(c.high_mark,10);
            assert_eq!(c.head,Cursor{ cycle: 1, offset: 5 });
        }
    }
}

/* NOTES

A.  Why wait on a write lock for this check (rather than a read)?

    Acquiring a lock twice in the same function is expensive. (assumed)

    This check is important when shutting down the channel. Sometimes it's
    worth acquiring a read lock because it allows one to skip enough write
    lock acquisitions.  That is, the average number of locks acquired per call
    becomes ~1.  That's not the case here.

B.  It's possible to delay computation of the pointer till it's needed. It
    doesn't need to be computed here. Delaying would save some space in the
    Sender struct but that's not important.  It also requires a read lock in
    rust.
 */
