use std::{
    collections::{btree_set::Intersection, HashSet},
    ops::Deref,
    sync::{mpsc::channel, Arc},
};

use log::{info, trace};
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
        let cur = channel.inner.lock().reads.beg;
        Receiver { channel, cur }
    }

    pub fn channel(&self) -> &Arc<Channel> {
        &self.channel
    }

    pub fn is_open(&self) -> bool {
        let ch = self.channel.inner.lock();
        ch.is_accepting_writes || self.cur != ch.writes.beg
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

        let (cur, ptr) = {
            let mut ch = self.channel.inner.lock();

            let w = ch.writes.beg;
            let end = ch.reads.end;

            assert!(self.cur <= w, "read cur:{} - {}", self.cur, ch);
            assert!(w.cycle - self.cur.cycle <= 2, "w:{} r:{}", w, self.cur);
            {
                let reads = Interval {
                    beg: self.cur,
                    end,
                    high_mark: ch.reads.high_mark,
                };
                if reads.len() == 0 {
                    // The read pos is at the min writer pos.  There is no data
                    // available.

                    // TODO: ? block until data is available. Return None on shutdown
                    // Original design doesn't block on map
                    return None;
                }
            }

            let cur: Interval = if self.cur.cycle == end.cycle {
                // same cycle case
                assert_ne!(end.offset, self.cur.offset);
                let cur = Interval {
                    beg: self.cur,
                    end,
                    high_mark: None,
                };
                assert_eq!(cur.len(), end.offset - self.cur.offset);
                cur
            } else {
                assert!(self.cur.cycle < w.cycle);
                let high_mark = ch
                    .reads.high_mark
                    .expect(&format!("cur: {}, ch:{}", self.cur, ch));
                // read_head is in the next cycle
                if self.cur.offset == high_mark {
                    // already at high, wrap
                    let cur = Interval {
                        beg: BegCursor {
                            cycle: self.cur.cycle + 1,
                            offset: 0,
                        },
                        end,
                        high_mark: ch.reads.high_mark,
                    };
                    assert_eq!(cur.len(), end.offset);
                    cur
                } else {
                    // take remainder on this cycle
                    let cur = Interval {
                        beg: self.cur,
                        end: EndCursor {
                            cycle: self.cur.cycle,
                            offset: high_mark,
                        },
                        high_mark: None,
                    };
                    assert_eq!(cur.len(), high_mark - self.cur.offset);
                    cur
                }
            };
            assert!(cur.end<=ch.reads.end,"cur:{} ch:{}",cur,ch);
            let ptr = unsafe { ch.ptr.as_ptr().offset(cur.beg.offset) as *const _ };

            // Remove should precede insert since it's a noop if there's
            // something missing.
            //
            // Consider, self.cur starts at A. Outstanding={A:1}
            // 1. Retrieve interval A-B: self.cur=B, outstanding={A:1}
            // 2. Retrieve interval B-C: self.cur=C, outstanding={A:1,B:1}
            //
            // Doing the insert before remove, would result in outstanding={A:1}
            // on step 2.
            ch.outstanding_reads.insert(cur.beg);
            ch.outstanding_reads.remove(&self.cur);
            (cur, ptr)
        };
        self.cur = cur.end.to_beg(cur.high_mark);
        assert!(cur.len() > 0);
        if cur.len() > 0 {
            Some(Region {
                owner: self,
                cur,
                buf: unsafe { std::slice::from_raw_parts(ptr, cur.len() as _) },
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
        ch.outstanding_reads.insert(interval.end.to_beg(interval.high_mark));
        let c0 = ch.reads.beg.cycle;
        ch.reads.beg = *ch
            .outstanding_reads
            .min()
            .unwrap_or(&interval.end.to_beg(interval.high_mark));
        let c1 = ch.reads.beg.cycle;
        if c1 > c0 {
            trace!("unset {} {} {:?} cur:{} int:{} ch.reads:{:?}", c0, c1,ch.reads,self.cur, interval,ch.outstanding_reads);
            ch.reads.high_mark = None;
        }
        self.channel.space_available.notify_all();
    }
}
