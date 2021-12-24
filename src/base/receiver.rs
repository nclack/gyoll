use std::{
    ops::Deref,
    sync::{mpsc::channel, Arc},
};

use parking_lot::RwLock;

use super::{channel::RawChannel, cursor::Cursor, region::Region, Channel};

pub struct Receiver {
    channel: Arc<RwLock<RawChannel>>,
    cur: Cursor,
}

unsafe impl Send for Receiver {}
unsafe impl Sync for Receiver {}

// TODO: disallow overlapping maps. Region must be unmapped first.

impl Receiver {
    pub(crate) fn new(channel: &Channel) -> Self {
        Receiver {
            channel: channel.0.clone(),
            cur: Cursor::zero(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.channel.read().is_accepting_writes
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

        let reg = {
            let mut c = self.channel.write();

            let w = std::iter::once(&c.head)
                .chain(c.outstanding_writes.iter())
                .min()
                .unwrap();

            assert!(self.cur <= *w);
            assert!(w.cycle - self.cur.cycle <= 1);

            if self.cur == *w {
                // TODO: ? block until data is available. Return None on shutdown
                // Original design doesn't block on map
                None
            } else {
                let ptr = unsafe { c.ptr.as_ptr().offset(self.cur.offset) as *const _ };
                let (end, len) = if self.cur.cycle == w.cycle {
                    (w.clone(), w.offset - self.cur.offset) // same cycle case
                } else {
                    // writer is in the next cycle
                    if self.cur.offset == c.high_mark {
                        // already at high, wrap
                        (w.clone(), w.offset)
                    } else {
                        // take remaining in this cycle
                        (
                            Cursor {
                                cycle: self.cur.cycle,
                                offset: c.high_mark,
                            },
                            c.high_mark - self.cur.offset,
                        )
                    }
                };
                c.outstanding_reads.insert(end.clone());
                Some((end, ptr, len))
            }
        };
        match reg {
            Some((end, ptr, len)) => Some(Region {
                owner: self,
                end,
                buf: unsafe { std::slice::from_raw_parts(ptr, len as _) },
            }),
            None => None,
        }
    }

    pub(crate) fn release(&mut self, end: &Cursor) {
        let mut c = self.channel.write();
        c.outstanding_reads.remove(end);
        c.space_available.notify_all();
    }
}
