//! Like a HashSet but keeps counts of repeated keys.

use std::collections::{hash_map::Entry, HashMap};
use std::hash::Hash;

/// Like a HashSet but keeps counts of repeated keys.
#[derive(Debug)]
pub(crate) struct Counter<T> {
    inner: HashMap<T, usize>,
}

impl<T> Counter<T>
where
    T: Eq + Hash + Copy + Ord,
{
    pub(crate) fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    pub(crate) fn insert(&mut self, v: T) {
        *self.inner.entry(v).or_insert(0) += 1;
    }

    /// Silently ignores missing values
    pub(crate) fn remove(&mut self, v: &T) {
        if let Entry::Occupied(mut o) = self.inner.entry(*v) {
            if *o.get() <= 1 {
                o.remove_entry();
            } else {
                *o.get_mut() -= 1;
            }
        }
    }

    pub(crate) fn min(&self) -> Option<&T> {
        self.inner.keys().min()
    }

    pub(crate) fn max(&self) -> Option<&T> {
        self.inner.keys().max()
    }
}
