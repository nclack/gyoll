use std::{collections::btree_set::Intersection, fmt::Display};

#[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub(crate) struct BegCursor {
    pub(crate) cycle: isize,
    pub(crate) offset: isize,
}

#[derive(Default, Debug, Clone, Copy, Hash)]
pub(crate) struct EndCursor {
    pub(crate) cycle: isize,
    pub(crate) offset: isize,
}

#[derive(Hash, Clone, Copy, Eq, PartialEq, Debug, Default)]
pub(crate) struct Interval {
    /// The beginning of the region.
    pub(crate) beg: BegCursor,

    /// The end of the region.
    pub(crate) end: EndCursor,

    /// If set, the buffer wrapped around and this was the high water mark.
    pub(crate) high_mark: Option<isize>,
}

impl Interval {
    pub(crate) fn len(&self) -> isize {
        if self.end.cycle == self.beg.cycle {
            self.end.offset - self.beg.offset
        } else {
            assert!(self.end.cycle > self.beg.cycle, "{}", self);
            let high_mark = self.high_mark.unwrap();
            self.end.offset + self.high_mark.unwrap() - self.beg.offset
        }
    }
}


impl PartialOrd for Interval {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.beg.partial_cmp(&other.beg) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        self.end.partial_cmp(&other.end)
    }
}

impl Ord for Interval {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl Display for Interval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{beg}-{end} high:{high}",
            beg = self.beg,
            end = self.end,
            high = if let Some(h) = self.high_mark { h } else { -1 }
        )
    }
}

impl BegCursor {
    pub(crate) fn zero() -> Self {
        Self::default()
    }

    pub(crate) fn is_empty(&self, other: &EndCursor) -> bool {
        Interval {
            beg: *self,
            end: *other,
            high_mark: None,
        }
        .len()
            == 0
    }

    pub(crate) fn to_end(&self, high_mark: Option<isize>) -> EndCursor {
        // TODO: change arg to take an interval. Needs to be the interval the high_mark was pulled from
        let (offset, cycle) = if let Some(high_mark) = high_mark {
            assert_eq!(self.offset, 0);
            (high_mark, self.cycle - 1)
        } else {
            (self.offset, self.cycle)
        };
        EndCursor { offset, cycle }
    }
}

impl EndCursor {
    pub(crate) fn zero() -> Self {
        Self::default()
    }

    /// Returns the next contiguous region of size `amount` assuming this this
    /// cursor points into a circular buffer of size `capacity`.
    pub(crate) fn next_region(&self, amount: usize, capacity: usize) -> Interval {
        let amount = amount as isize;
        let capacity = capacity as isize;
        if self.offset + amount > capacity {
            // Not enough space => wrap to next cycle
            let cycle = self.cycle + 1;
            let beg = BegCursor { offset: 0, cycle };
            let end = EndCursor {
                offset: amount,
                cycle,
            };
            Interval {
                beg,
                end,
                high_mark: Some(self.offset),
            }
        } else {
            // Enough space => this cycle
            // This beg will never be at the end point for the cycle
            // (the high mark or the capacity)
            // precisely because there is space left.
            let beg = BegCursor {
                offset: self.offset,
                cycle: self.cycle,
            };
            let end = EndCursor {
                offset: self.offset + amount,
                ..*self
            };
            Interval {
                beg,
                end,
                high_mark: None,
            }
        }
    }

    pub(crate) fn to_beg(&self, high_mark: Option<isize>) -> BegCursor {
        if let Some(high_mark) = high_mark {
            if high_mark == self.offset {
                return BegCursor {
                    offset: 0,
                    cycle: self.cycle + 1,
                };
            }
        }
        BegCursor {
            offset: self.offset,
            cycle: self.cycle,
        }
    }
}

impl PartialEq for EndCursor {
    fn eq(&self, other: &Self) -> bool {
        self.cycle == other.cycle && self.offset == other.offset
    }
}

impl Eq for EndCursor {}

impl PartialOrd for EndCursor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.cycle.partial_cmp(&other.cycle) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        self.offset.partial_cmp(&other.offset)
    }
}

impl Ord for EndCursor {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl Display for BegCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}({})", self.offset, self.cycle)
    }
}

impl Display for EndCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}({})", self.offset, self.cycle)
    }
}

#[cfg(test)]
mod tests {
    use super::{BegCursor, EndCursor};

    #[test]
    #[rustfmt::skip]
    fn cursor_order() {
        assert!(
            BegCursor {offset: 0,cycle: 1} > BegCursor {offset: 100,cycle: 0}
        );
        assert!(
            EndCursor {offset: 100, cycle: 0} < EndCursor {offset: 0, cycle: 1} 
        );
    }

    #[test]
    #[rustfmt::skip]
    fn cursor_inc_region() {
        let c = EndCursor{ cycle: 0, offset: 0 };
        
        // no wrap
        let inc = c.next_region(10, 20);
        assert_eq!(inc.beg, BegCursor{offset:0,cycle:0});
        assert_eq!(inc.end, EndCursor{offset:10,cycle:0});

        // no wrap - exact fit
        let inc = inc.end.next_region(10, 20);
        assert_eq!(inc.beg, BegCursor{offset:10,cycle:0});
        assert_eq!(inc.end, EndCursor{offset:20,cycle:0});
        
        // wrap
        let inc = inc.end.next_region(15, 20);
        assert_eq!(inc.beg, BegCursor{offset:0,cycle:1});
        assert_eq!(inc.end, EndCursor{offset:15,cycle:1});

        // wrap - can't fit
        let inc = inc.end.next_region(15, 20);
        assert_eq!(inc.beg, BegCursor{offset:0,cycle:2});
        assert_eq!(inc.end, EndCursor{offset:15,cycle:2});
    }
}
