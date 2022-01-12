use std::{
    collections::HashSet,
    ops::Deref,
    sync::{mpsc::channel, Arc},
};

use parking_lot::{RwLock, RwLockUpgradableReadGuard};

use super::{
    channel::{Channel, RawChannel},
    cursor::Cursor,
    region::Region,
};

pub struct Receiver {
    channel: Arc<Channel>,

    /// The read position
    /// This is often the beginning of the next read region.
    cur: Cursor,
}

unsafe impl Send for Receiver {}
unsafe impl Sync for Receiver {}

impl Receiver {
    pub(crate) fn new(channel: Arc<Channel>) -> Self {
        let cur = {
            let mut ch = channel.inner.lock();
            let cur = *ch.min_write_pos();
            ch.outstanding_reads.insert(cur);
            cur
        };
        Receiver { channel, cur }
    }

    pub fn channel(&self) -> &Arc<Channel> {
        &self.channel
    }

    pub fn is_open(&self) -> bool {
        let ch = self.channel.inner.lock();
        ch.is_accepting_writes || self.cur != ch.head
    }

    pub fn next(&mut self) -> Option<Region> {
        // Assert
        //  - write point >= read point
        //  - write point is < one cycle ahead
        //
        // Cases:
        //  - write point is on same cycle
        //      Create a region [cur,w]
        //  - write point is on next cycle
        //      Create a region [cur,high]
        //

        let (beg, end, ptr, len) = {
            let mut ch = self.channel.inner.lock();

            let w = ch.min_write_pos();

            assert!(self.cur <= *w, "w:{} r:{}", w, self.cur);
            assert!(w.cycle - self.cur.cycle <= 1, "w:{} r:{}", w, self.cur);
            if self.cur == *w {
                // The read pos is at the min writer pos.  There is no data
                // available.

                // TODO: ? block until data is available. Return None on shutdown
                // Original design doesn't block on map
                return None;
            }

            let (beg, end, len) = if self.cur.cycle == w.cycle {
                // same cycle case
                assert_ne!(w.offset, self.cur.offset);
                (self.cur, *w, w.offset - self.cur.offset)
            } else {
                assert_eq!(self.cur.cycle + 1, w.cycle);
                // writer is in the next cycle

                let res=if self.cur.offset == ch.high_mark {
                    // already at high, wrap
                    (
                        Cursor {
                            cycle: self.cur.cycle + 1,
                            offset: 0,
                        },
                        *w,
                        w.offset,
                    )
                } else {
                    // take remainder on this cycle
                    (
                        self.cur,
                        Cursor {
                            cycle: self.cur.cycle + 1,
                            offset: 0,
                        },
                        ch.high_mark - self.cur.offset,
                    )
                };
                ch.outstanding_reads.remove(&self.cur);
                res
            };
            let ptr = unsafe { ch.ptr.as_ptr().offset(beg.offset) as *const _ };
            ch.outstanding_reads.insert(beg);
            (beg, end, ptr, len)
        };
        self.cur = end;
        if len > 0 {
            Some(Region {
                owner: self,
                beg,
                end,
                buf: unsafe { std::slice::from_raw_parts(ptr, len as _) },
            })
        } else {
            return None;
        }
    }

    pub(crate) fn unreserve(&mut self, beg: &Cursor, end: &Cursor) {
        let mut ch = self.channel.inner.lock();

        ch.outstanding_reads.remove(beg);
        ch.outstanding_reads.insert(*end);
        println!(
            "Release read at {}-{} n: {:?}",
            beg, end, ch.outstanding_reads
        );

        self.channel.space_available.notify_all();
    }
}
