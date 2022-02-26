use std::{
    collections::HashSet,
    ops::Deref,
    sync::{mpsc::channel, Arc},
};

use parking_lot::{RwLock, RwLockUpgradableReadGuard};

use crate::base::cursor::EndCursor;

use super::{
    channel::{Channel, RawChannel},
    cursor::{BegCursor, Interval},
    region::Region,
};

pub struct Receiver {
    channel: Arc<Channel>,

    /// The read position
    /// This is often the beginning of the next read region.
    cur: BegCursor,
}

unsafe impl Send for Receiver {}
unsafe impl Sync for Receiver {}

impl Receiver {
    pub(crate) fn new(channel: Arc<Channel>) -> Self {
        let cur = channel.inner.lock().read_tail;
        Receiver { channel, cur }
    }

    pub fn channel(&self) -> &Arc<Channel> {
        &self.channel
    }

    pub fn is_open(&self) -> bool {
        let ch = self.channel.inner.lock();
        ch.is_accepting_writes || self.cur != ch.write_tail
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

        let (cur, ptr, len) = {
            let mut ch = self.channel.inner.lock();

            
            let w = ch.write_tail;
            let end = ch.read_head;

            assert!(self.cur <= w, "w:{} r:{}", w, self.cur);
            assert!(w.cycle - self.cur.cycle <= 2, "w:{} r:{}", w, self.cur);
            if self.cur == w {
                // The read pos is at the min writer pos.  There is no data
                // available.

                // TODO: ? block until data is available. Return None on shutdown
                // Original design doesn't block on map
                return None;
            }

            let (cur, len): (Interval, isize) = if self.cur.cycle == end.cycle {
                // same cycle case
                assert_ne!(w.offset, self.cur.offset);
                (
                    Interval {
                        beg: self.cur,
                        end,
                        high_mark: None,
                    },
                    w.offset - self.cur.offset,
                )
            } else {
                assert!(self.cur.cycle < end.cycle);
                let high_mark=ch.high_mark.unwrap();
                // read_head is in the next cycle
                if self.cur.offset == high_mark {
                    // already at high, wrap
                    (
                        Interval {
                            beg: BegCursor {
                                cycle: self.cur.cycle + 1,
                                offset: 0,
                            },
                            end,
                            high_mark: ch.high_mark 
                        },
                        w.offset,
                    )
                } else {
                    // take remainder on this cycle
                    (
                        Interval {
                            beg: self.cur,
                            end: EndCursor {
                                cycle: self.cur.cycle,
                                offset: high_mark,
                            },
                            high_mark: None,
                        },
                        high_mark - self.cur.offset,
                    )
                }
            };
            let ptr = unsafe { ch.ptr.as_ptr().offset(cur.beg.offset) as *const _ };
            ch.outstanding_reads.insert(cur.beg);
            (cur, ptr, len)
        };
        self.cur = cur.end.to_beg(cur.high_mark);
        if len > 0 {
            Some(Region {
                owner: self,
                cur,
                buf: unsafe { std::slice::from_raw_parts(ptr, len as _) },
            })
        } else {
            return None;
        }
    }

    pub(crate) fn unreserve(&mut self, interval: &Interval) {
        // Remove the region and update the read_tail. If this is the last 
        // region outstanding then the read_tail corresponds to the end, 
        // otherwise it's just the min over all outstanding reads.
        let mut ch = self.channel.inner.lock();
        ch.outstanding_reads.remove(&interval.beg);
        ch.read_tail = *ch
            .outstanding_reads
            .min()
            .unwrap_or(&interval.end.to_beg(interval.high_mark));
        if ch.read_tail.cycle==ch.read_head.cycle {
            ch.high_mark=None;
        }
        self.channel.space_available.notify_all();
    }
}
