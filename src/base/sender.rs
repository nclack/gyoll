use std::{mem::size_of_val, sync::Arc};

use parking_lot::lock_api::RawRwLockUpgrade;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};

use super::{
    channel::{Channel, RawChannel},
    cursor::Cursor,
    region::MutRegion,
};

pub struct Sender {
    channel: Arc<Channel>,
}

unsafe impl Send for Sender {}
unsafe impl Sync for Sender {}

impl Sender {
    pub(crate) fn new(channel: Arc<Channel>) -> Self {
        Sender { channel }
    }

    pub fn channel(&self) -> &Arc<Channel> {
        &self.channel
    }

    /// Reserves a mutable region of the channel.
    ///
    /// Blocks until a region is available.
    ///
    /// Returns None when the channel is unwritable or `nbytes` exceeds the
    /// channels `capacity`.
    pub fn map(&mut self, nbytes: usize) -> Option<MutRegion> {
        let (cur, ptr) = {
            let mut ch = self.channel.inner.lock();

            if nbytes > ch.capacity || !ch.is_accepting_writes {
                return None;
            }

            let inc = ch.head.next_region(nbytes, ch.capacity);

            fn collide(w: &Cursor, r: &Cursor) -> bool {
                // On the same cycle, there can be no collision bc r<=w.
                // Otherwise,
                w.cycle > r.cycle && w.offset >= r.offset
            }

            while collide(&inc.end, ch.min_read_pos()) && ch.is_accepting_writes {
                // println!("     - {} r:{}",inc,ch.min_read_pos());
                self.channel.space_available.wait(&mut ch);
                // println!("exit - {} r:{}",inc,ch.min_read_pos());
            }

            if !ch.is_accepting_writes {
                return None;
            }

            // At this point there's space available so we're ready to reserve
            // the region.

            ch.outstanding_writes.insert(inc.beg);
            ch.head = inc.end;
            if let Some(high_mark) = inc.high_mark {
                ch.high_mark = high_mark;
            }
            let ptr = unsafe { ch.ptr.as_ptr().offset(inc.beg.offset) };
            (inc.beg, ptr)
        };

        // Finally, construct the region
        let buf = unsafe { std::slice::from_raw_parts_mut(ptr, nbytes) };
        Some(MutRegion {
            owner: self,
            cur,
            buf,
        })
    }

    pub(crate) fn unreserve(&self, beg: &Cursor) {
        self.channel.inner.lock().outstanding_writes.remove(beg);
    }
}

#[cfg(test)]
mod test {
    use crate::base::{channel, cursor::Cursor, region::MutRegion};

    #[test]
    #[rustfmt::skip]
    fn send_wrap_behavior() {
        let (mut tx,mut rx)=channel(13);
        {
            let reg = tx.map(5).unwrap();
            assert_eq!(reg.cur,Cursor { cycle: 0, offset: 0 });
        }
        {
            let reg = tx.map(5).unwrap();
            assert_eq!(reg.cur,Cursor { cycle: 0, offset: 5 });
        }
        while rx.next().is_some() {}; // drain so we can continue
        {
            let reg = tx.map(5).unwrap();
            assert_eq!(reg.cur,Cursor { cycle: 1, offset: 0 });
        }
        {
            let c=tx.channel.inner.lock();
            assert_eq!(c.high_mark,10);
            assert_eq!(c.head,Cursor{ cycle: 1, offset: 5 });
        }
    }
}
