use std::fmt::Display;

#[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub(crate) struct Cursor {
    pub(crate) cycle: isize,
    pub(crate) offset: isize,
}

pub(crate) struct Increment {
    /// The beginning of the region.
    pub(crate) beg: Cursor,

    /// The end of the region.
    pub(crate) end: Cursor,

    /// If set, the buffer wrapped around and this was the high water mark.
    pub(crate) high_mark: Option<isize>,
}

impl Display for Increment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(high) = self.high_mark {
            write!(
                f,
                "{cycle:6}: {beg:6}-{end:6} high:{high}",
                cycle = self.beg.cycle,
                beg = self.beg.offset,
                end = self.end.offset,
                high = high
            )
        } else {
            write!(
                f,
                "{cycle:6}: {beg:6}-{end:6}",
                cycle = self.beg.cycle,
                beg = self.beg.offset,
                end = self.end.offset,
            )
        }
    }
}

impl Cursor {
    pub(crate) fn zero() -> Self {
        Self {
            ..Default::default()
        }
    }

    /// Returns the next contiguous region of size `amount` assuming this this
    /// cursor points into a circular buffer of size `capacity`.
    pub(crate) fn next_region(&self, amount: usize, capacity: usize) -> Increment {
        let amount = amount as isize;
        let capacity = capacity as isize;
        if self.offset + amount > capacity {
            let cycle = self.cycle + 1;
            let beg = Cursor { offset: 0, cycle };
            let end = Cursor {
                offset: amount,
                cycle,
            };
            Increment {
                beg,
                end,
                high_mark: Some(self.offset),
            }
        } else {
            let end = Cursor {
                offset: self.offset + amount,
                cycle: self.cycle,
            };
            Increment {
                beg: *self,
                end,
                high_mark: None,
            }
        }
    }
}

impl Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}({})", self.offset, self.cycle)
    }
}

#[cfg(test)]
mod tests {
    use crate::base::cursor::Cursor;

    #[test]
    #[rustfmt::skip]
    fn cursor_order() {
        assert!(
            Cursor {offset: 0,cycle: 1} > Cursor {offset: 100,cycle: 0}
        )
    }

    #[test]
    #[rustfmt::skip]
    fn cursor_inc_region() {
        let c = Cursor::zero();
        
        // no wrap
        let inc = c.next_region(10, 20);
        assert_eq!(inc.beg, Cursor{offset:0,cycle:0});
        assert_eq!(inc.end, Cursor{offset:10,cycle:0});

        // no wrap - exact fit
        let inc = inc.end.next_region(10, 20);
        assert_eq!(inc.beg, Cursor{offset:10,cycle:0});
        assert_eq!(inc.end, Cursor{offset:20,cycle:0});
        
        // wrap
        let inc = inc.beg.next_region(15, 20);
        assert_eq!(inc.beg, Cursor{offset:0,cycle:1});
        assert_eq!(inc.end, Cursor{offset:15,cycle:1});

        // wrap - can't fit
        let inc = inc.end.next_region(15, 20);
        assert_eq!(inc.beg, Cursor{offset:0,cycle:2});
        assert_eq!(inc.end, Cursor{offset:15,cycle:2});
    }
}
