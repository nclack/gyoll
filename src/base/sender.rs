use std::{mem::size_of_val, sync::Arc};

use log::trace;
use parking_lot::lock_api::RawRwLockUpgrade;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};

use crate::base::cursor::EndCursor;

use super::{
    channel::{Channel, RawChannel},
    cursor::BegCursor,
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

            let (inc, wrapped) = ch.write_head.next_region(nbytes, ch.capacity);

            // Reserve the region even though we haven't fully acquired it yet.
            ch.outstanding_writes.insert(inc.beg);
            if wrapped {
                ch.high_mark = inc.end.wrap;
            }
            ch.write_head = inc.end;

            fn collide(w: &EndCursor, r: &BegCursor) -> bool {
                // On the same cycle, there can be no collision bc enforce r<=w elsewhere.
                // Otherwise,
                w.cycle > r.cycle && (w.offset > r.offset || w.cycle > r.cycle + 1)
                // The w.cycle>r.cycle+1 case handles when the first unread
                // byte is hanging off the end of the cycle.
            }

            while collide(&inc.end, &ch.min_read_pos()) && ch.is_accepting_writes {
                trace!("     - {} r:{}", inc, ch.min_read_pos());
                self.channel.space_available.wait(&mut ch);
                trace!("exit - {} r:{}", inc, ch.min_read_pos());
            }
            assert!(
                inc.beg.cycle - ch.min_read_pos().cycle < 3,
                "inc:{} r:{}",
                inc,
                ch.min_read_pos()
            );

            if !ch.is_accepting_writes {
                // don't need to fix the write head bc we won't be doing more writes
                ch.outstanding_writes.remove(&inc.beg);
                return None;
            }

            // At this point there's space available so we're ready to reserve
            // the region.
            ch.read_head = ch.read_head.max(inc.end);

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

    pub(crate) fn unreserve(&self, beg: &BegCursor) {
        let mut ch = self.channel.inner.lock();
        ch.outstanding_writes.remove(beg);
    }
}

#[cfg(test)]
mod test {
    use crate::base::{
        channel,
        cursor::{BegCursor, EndCursor},
        region::MutRegion,
    };

    #[test]
    #[rustfmt::skip]
    fn send_wrap_behavior() {
        let (mut tx,mut rx)=channel(13);
        {
            let reg = tx.map(5).unwrap();
            assert_eq!(reg.cur,BegCursor { cycle: 0, offset: 0 });
        }
        {
            let reg = tx.map(5).unwrap();
            assert_eq!(reg.cur,BegCursor { cycle: 0, offset: 5 });
        }
        while rx.next().is_some() {}; // drain so we can continue
        {
            let reg = tx.map(5).unwrap();
            assert_eq!(reg.cur,BegCursor { cycle: 1, offset: 0 });
        }
        {
            let c=tx.channel.inner.lock();
            assert_eq!(c.high_mark,10);
            assert_eq!(c.write_head,EndCursor{cycle:1,offset:5, wrap: 10 });
        }
    }
}
