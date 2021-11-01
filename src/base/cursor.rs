#[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub(crate) struct Cursor {
    pub(crate) cycle: isize,
    pub(crate) offset: isize,
}

impl Cursor {
    pub(crate) fn zero() -> Self {
        Self {
            ..Default::default()
        }
    }

    /// Returns the next region of size `amount` after `self` in a circular
    /// buffer of size `capacity`.
    ///
    /// When the region doesn't wrap around the end of the buffer, it is just
    /// the interval `[self,self+amount]`.  When it does wrap, the region
    /// skips to the next cycle.
    pub(crate) fn inc_region(&self, amount: usize, capacity: usize) -> (Cursor, Cursor) {
        let amount = amount as isize;
        let capacity = capacity as isize;
        if self.offset + amount > capacity {
            let cycle = self.cycle + 1;
            (
                Cursor { offset: 0, cycle },
                Cursor {
                    offset: amount,
                    cycle,
                },
            )
        } else {
            (
                self.clone(),
                Cursor {
                    offset: self.offset + amount,
                    cycle: self.cycle,
                },
            )
        }
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
        let (beg, end) = c.inc_region(10, 20);
        assert_eq!(beg, Cursor{offset:0,cycle:0});
        assert_eq!(end, Cursor{offset:10,cycle:0});

        // no wrap - exact fit
        let (beg, end) = end.inc_region(10, 20);
        assert_eq!(beg, Cursor{offset:10,cycle:0});
        assert_eq!(end, Cursor{offset:20,cycle:0});
        
        // wrap
        let (beg, end) = beg.inc_region(15, 20);
        assert_eq!(beg, Cursor{offset:0,cycle:1});
        assert_eq!(end, Cursor{offset:15,cycle:1});
    }
}
