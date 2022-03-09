use std::{mem::size_of_val, sync::Arc};

use log::{info, trace};
use parking_lot::lock_api::RawRwLockUpgrade;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};

use crate::base::cursor::EndCursor;

use super::cursor::Interval;
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
    pub(super) fn new(channel: Arc<Channel>) -> Self {
        Sender { channel }
    }

    /// Get a reference to the channel.
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

            let prev_write_head = ch.writes.end;
            let inc = ch.writes.end.next_region(nbytes, ch.capacity);

            // Reserve the region even though we haven't fully acquired it yet.
            //
            // The new region will be > any outstanding_write, but if there
            // are none, we need to update the write_tail. This will assist
            // with wrapping around a cycle sometimes.
            if ch.outstanding_writes.is_empty() {
                ch.writes.beg = inc.beg;
            }
            ch.writes.end = inc.end;
            ch.outstanding_writes.insert(inc);

            fn collide(w: &EndCursor, r: &BegCursor) -> bool {
                // On the same cycle, there can be no collision bc enforce
                // r<=w elsewhere. Otherwise,
                w.cycle > r.cycle && (w.offset > r.offset || w.cycle > r.cycle + 1)
                // The w.cycle>r.cycle+1 case handles when the first unread
                // byte is hanging off the end of the cycle.
            }

            while collide(&inc.end, &ch.reads.beg) && ch.is_accepting_writes {
                trace!("     - {} r:{}", inc, ch.reads.beg);
                self.channel.space_available.wait(&mut ch);
                trace!("exit - {} r:{}", inc, ch.reads.beg);
            }

            if !ch.is_accepting_writes {
                // Once the channel stops accepting writes, it cannot be
                // reopened. There may be some outstanding mutable regions.
                // When these get released they'll update the write_tail
                // appropriately. The write_head should move back to the start
                // of the uncommitted region. This is just a min over all the
                // outstanding prev_write_heads.
                //
                // While threads are waking the write_head is in an undefined
                // state. No write's are incoming. The only dependency to
                // worry about is write_tail, which defaults to write_head
                // when there are no outstanding regions. But that's precisely
                // the point where write_head is guaranteed to be correct.
                ch.outstanding_writes.remove(&inc);
                ch.writes.end = ch.writes.end.min(prev_write_head);
                return None;
            }

            assert!(
                inc.beg.cycle - ch.reads.beg.cycle < 2,
                "inc:{} r:{} ch:{}",
                inc,
                ch.reads.beg,
                ch
            );

            // At this point there's space available so we're ready to reserve
            // the region.

            // If this increment causes a wrap, then record that as the high
            // mark for the "writes" interval. Later this will be used to set
            // the high mark for the "reads".
            if inc.high_mark.is_some() {
                trace!("latch {}", inc);
                ch.writes.high_mark = inc.high_mark;
            }

            let ptr = unsafe { ch.ptr.as_ptr().offset(inc.beg.offset) };
            (inc, ptr)
        };

        // Finally, construct the region
        let buf = unsafe { std::slice::from_raw_parts_mut(ptr, nbytes) };
        Some(MutRegion {
            owner: self,
            cur,
            buf,
        })
    }

    pub(super) fn unreserve(&self, interval: &Interval) {
        let mut ch = self.channel.inner.lock();
        ch.outstanding_writes.remove(interval);

        let mn = ch.outstanding_writes.iter().min().map(|e| *e);

        // The outstanding_writes includes the uncommitted writes, so if it's
        // empty the high_mark should be the last interval removed.
        ch.writes.beg = mn.map(|e| e.beg).unwrap_or(interval.end.into());

        // read_head should default to write_tail when there are no
        // outstanding_writes. But write_tail defaults to interval.end in that
        // case. Take that shortcut below to avoid switching the sense of the
        // endpoint.
        let c0 = ch.reads.end.cycle;
        ch.reads.end = mn
            .map(|e| e.beg.to_end(e.high_mark))
            .unwrap_or(interval.end);
        let c1 = ch.reads.end.cycle;

        assert!(
            BegCursor::from(ch.reads.end)
                >= *ch.outstanding_reads.max().unwrap_or(&ch.reads.end.into())
        );

        // update high mark for when read_head crosses a cycle boundary
        if c1 > c0 {
            trace!(
                "set {} {} cycle: {} high:{:?}",
                c0,
                c1,
                ch.reads.beg.cycle,
                ch.writes.high_mark
            );
            ch.reads.high_mark = ch.writes.high_mark;
            ch.writes.high_mark = None;
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        sync::{
            atomic::{AtomicBool, AtomicU8, Ordering},
            Arc,
        },
        thread::{sleep, spawn},
        time::Duration,
    };

    use log::info;

    use crate::base::{
        channel,
        cursor::{BegCursor, EndCursor, Interval},
        region::MutRegion,
    };

    #[test]
    #[rustfmt::skip]
    fn send_wrap_behavior() {
        let done=Arc::new(AtomicBool::new(false));

        let timer={
            let done=done.clone();
            spawn(move || {
                sleep(Duration::from_secs_f32(1.0));
                assert!(done.load(Ordering::SeqCst),"Failed to terminate in time.");
            })
        };

        let (mut tx,mut rx)=channel(13);
        info!("write 5");
        {
            let reg = tx.map(5).unwrap();
            assert_eq!(reg.cur,Interval{ 
                beg: BegCursor { cycle: 0, offset: 0 },
                end: EndCursor { cycle: 0, offset: 5 },
                high_mark: None
            });
        }
        info!("write 5");
        {
            let reg = tx.map(5).unwrap();
            assert_eq!(reg.cur,Interval{ 
                beg: BegCursor { cycle: 0, offset: 5 },
                end: EndCursor { cycle: 0, offset:10 },
                high_mark: None
            });
        }

        info!("read all");
        while rx.next().is_some() {}; // drain so we can continue

        info!("here");
        {
            let reg = tx.map(5).unwrap();
            assert_eq!(reg.cur,Interval{ 
                beg: BegCursor { cycle: 1, offset: 0 },
                end: EndCursor { cycle: 1, offset: 5 },
                high_mark: Some(10)
            });
        }
        info!("here");
        {
            let c=tx.channel.inner.lock();
            assert_eq!(c.reads.high_mark,Some(10));
            assert_eq!(c.writes.end,EndCursor{cycle:1,offset:5});
        }

        done.store(true, Ordering::SeqCst);
    }
}

// TODO: test channel drain, outstanding writes etc
