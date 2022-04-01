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
    cur: EndCursor,
}

unsafe impl Send for Receiver {}
unsafe impl Sync for Receiver {}

impl Receiver {
    pub(crate) fn new(channel: Arc<Channel>) -> Self {
        let cur = {
            let mut ch = channel.inner.lock();
            let cur = ch.reads.beg.to_end(None);
            ch.outstanding_reads.insert(cur.into());
            cur
        };
        Receiver { channel, cur }
    }

    pub fn channel(&self) -> &Arc<Channel> {
        &self.channel
    }

    pub fn is_open(&self) -> bool {
        let ch = self.channel.inner.lock();
        ch.is_accepting_writes || self.cur != ch.reads.end
    }

    pub fn next(&mut self) -> Option<Region> {
        let (interval, ptr) = {
            let mut ch = self.channel.inner.lock();

            // FIXME: Got
            // 'R1' panicked at 'cur:61441(11048) reads:61441(11048)-4895(11049) high:-1'
            // 'R1' panicked at 'cur:61440(10762) reads:61440(10762)-45073(10763) high:-1'
            //
            // Shouldn't high mark be set here. I'm inclined to think this is mostly a fine state
            // but that high mark should still be set.
            assert!(
                (ch.reads.high_mark.is_some() && ch.reads.end.cycle == ch.reads.beg.cycle + 1)
                    || (ch.reads.high_mark.is_none() && ch.reads.end.cycle == ch.reads.beg.cycle),
                "cur:{} reads:{} ch:{:?}",
                self.cur,
                ch.reads,
                ch
            );
            assert!(
                ch.reads.beg <= self.cur.into() && self.cur <= ch.reads.end,
                "cur:{} reads:{} ch:{:?}",
                self.cur,
                ch.reads,
                ch
            );

            // ^^^^ 
            // thread 'R1' panicked at 'cur:61457(10752) reads:61457(10752)-4130(10753) high:-1 
            // ch:RawChannel { 
                // ptr: 0x150008000, capacity: 65536, is_accepting_writes: false, 
                // writes: Interval { beg: BegCursor { cycle: 10753, offset: 4130 }, end: EndCursor { cycle: 10752, offset: 61457 }, high_mark: None }, 
                // reads: Interval { beg: BegCursor { cycle: 10752, offset: 61457 }, end: EndCursor { cycle: 10753, offset: 4130 }, high_mark: None }, 
                // outstanding_writes: {}, outstanding_reads: Counter { inner: {BegCursor { cycle: 10752, offset: 61457 }: 3} } }', 
            // src/base/receiver.rs:59:13

            // Only wrap if there's a cycle difference.
            //
            // This is particularly important for the case where `reads.beg`
            // and `reads.end` are in different cycles, but the `self.cur` is
            // at `reads.end` and that happens to correspond to the `high_mark`.
            let beg = self.cur.to_beg(if self.cur.cycle == ch.reads.end.cycle {
                None
            } else {
                ch.reads.high_mark
            });

            // Compute the interval to read
            // It will never straddle the cycle boundary so the high_mark
            // should never be set.
            let interval = if beg.cycle == ch.reads.end.cycle {
                Interval {
                    beg,
                    end: ch.reads.end,
                    high_mark: None,
                }
            } else {
                assert_eq!(ch.reads.beg.cycle, beg.cycle, "beg:{} ch:{}", beg, ch);
                assert!(ch.reads.high_mark.is_some(), "beg:{} ch:{}", beg, ch);
                let high_mark = ch.reads.high_mark.unwrap();
                Interval {
                    beg,
                    end: EndCursor {
                        cycle: ch.reads.beg.cycle,
                        offset: high_mark,
                    },
                    high_mark: None,
                }
            };
            assert!(interval.high_mark.is_none());
            if interval.len() == 0 {
                return None;
            }

            let ptr = unsafe { ch.ptr.as_ptr().offset(interval.beg.offset) as *const _ };

            ch.outstanding_reads.insert(interval.beg);
            ch.outstanding_reads.remove(&self.cur.into());
            (interval, ptr)
        };
        self.cur = interval.end;

        assert!(interval.len() > 0);
        if interval.len() > 0 {
            Some(Region {
                owner: self,
                cur: interval,
                buf: unsafe { std::slice::from_raw_parts(ptr, interval.len() as _) },
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
        ch.outstanding_reads.insert(interval.end.into());
        let c0 = ch.reads.beg.cycle;
        let before = ch.reads;
        ch.reads.beg = *ch.outstanding_reads.min().unwrap_or(&interval.end.into());
        let c1 = ch.reads.beg.cycle;
        if c1 > c0 {
            trace!(
                "unset {} {} reads:{} cur:{} int:{} ch.outstanding_reads:{:?}",
                c0,
                c1,
                before,
                self.cur,
                interval,
                ch.outstanding_reads
            );
            ch.reads.high_mark = None;
        }
        self.channel.space_available.notify_all();
    }
}
