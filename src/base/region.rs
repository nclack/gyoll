use std::ops::{Deref, DerefMut};

use super::{
    cursor::{BegCursor, EndCursor, Interval},
    receiver::Receiver,
    sender::Sender,
};

//
//  MutRegion
//

pub struct MutRegion<'a> {
    pub(crate) owner: &'a mut Sender,
    pub(crate) cur: Interval,
    pub(crate) buf: &'a mut [u8],
}

impl<'a> AsMut<[u8]> for MutRegion<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.buf
    }
}

impl<'a> Deref for MutRegion<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.buf
    }
}

impl<'a> DerefMut for MutRegion<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buf
    }
}

impl<'a> Drop for MutRegion<'a> {
    fn drop(&mut self) {
        self.owner.unreserve(&self.cur);
    }
}

//
// Region
//

pub struct Region<'a> {
    pub(crate) owner: &'a mut Receiver,
    pub(crate) cur: Interval,
    pub(crate) buf: &'a [u8],
}

impl<'a> Region<'a> {
    // for debuggging
    pub fn cycle(&self)->isize {
        self.cur.beg.cycle
    }
}

impl<'a> Deref for Region<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.buf
    }
}

impl<'a> AsRef<[u8]> for Region<'a> {
    fn as_ref(&self) -> &[u8] {
        self.buf
    }
}

impl<'a> Drop for Region<'a> {
    fn drop(&mut self) {
        self.owner.unreserve(&self.cur);
    }
}
