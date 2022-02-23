use std::fmt::Display;

#[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub(crate) struct BegCursor {
    pub(crate) cycle: isize,
    pub(crate) offset: isize,
}

#[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub(crate) struct EndCursor {
    pub(crate) cycle: isize,
    pub(crate) offset: isize,
    pub(crate) wrap: isize,
}

pub(crate) struct Increment {
    /// The beginning of the region.
    pub(crate) beg: BegCursor,

    /// The end of the region.
    pub(crate) end: EndCursor,
}

impl Display for Increment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{cycle:6}: {beg:6}-{end:6} high:{high}",
            cycle = self.beg.cycle,
            beg = self.beg.offset,
            end = self.end.offset,
            high = self.end.wrap
        )
    }
}

impl BegCursor {
    pub(crate) fn zero() -> Self {
        Self::default()
    }

    pub(crate) fn to_end(self,wrap:isize)->EndCursor {
        if self.offset==0 {
            EndCursor{ cycle: self.cycle-1, offset:wrap, wrap:wrap}
        } else {
            EndCursor { cycle: self.cycle, offset: self.offset, wrap: wrap }
        }
    }

}

impl EndCursor {
    pub(crate) fn to_beg(self) -> BegCursor {
        let (cycle, offset) = if self.offset == self.wrap {
            (self.cycle + 1, 0)
        } else {
            (self.cycle, self.offset)
        };
        BegCursor { cycle, offset }
    }

    /// Returns the next contiguous region of size `amount` assuming this this
    /// cursor points into a circular buffer of size `capacity`.
    pub(crate) fn next_region(&self, amount: usize, capacity: usize) -> (Increment, bool) {
        let amount = amount as isize;
        let capacity = capacity as isize;
        if self.offset + amount > capacity {
            let cycle = self.cycle + 1;
            let beg = BegCursor { offset: 0, cycle };
            let end = EndCursor {
                offset: amount,
                cycle,
                wrap: self.offset,
            };
            (Increment { beg, end }, true)
        } else {
            let end = EndCursor {
                offset: self.offset + amount,
                ..*self
            };
            (Increment { beg: self.to_beg(), end }, false)
        }
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
        )
    }

    #[test]
    #[rustfmt::skip]
    fn cursor_inc_region() {
        let c = BegCursor::zero().to_end(20);
        
        // no wrap
        let (inc,wrapped) = c.next_region(10, 20);
        assert!(!wrapped);
        assert_eq!(inc.beg, BegCursor{offset:0,cycle:0});
        assert_eq!(inc.end, EndCursor{offset:10,cycle:0,wrap:20});

        // no wrap - exact fit
        let (inc,wrapped) = inc.end.next_region(10, 20);
        assert!(!wrapped);
        assert_eq!(inc.beg, BegCursor{offset:10,cycle:0});
        assert_eq!(inc.end, EndCursor{offset:20,cycle:0,wrap:20});
        
        // wrap
        let (inc,wrapped) = inc.end.next_region(15, 20);
        assert!(wrapped);
        assert_eq!(inc.beg, BegCursor{offset:0,cycle:1});
        assert_eq!(inc.end, EndCursor{offset:15,cycle:1,wrap:10});

        // wrap - can't fit
        let (inc,wrapped) = inc.end.next_region(15, 20);
        assert!(wrapped);
        assert_eq!(inc.beg, BegCursor{offset:0,cycle:2});
        assert_eq!(inc.end, EndCursor{offset:15,cycle:2,wrap:15});
    }
}
