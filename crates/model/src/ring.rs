//! Bounded FIFO. The capacity is a const generic so deserialisation (a plain JSON sequence)
//! knows the bound and a hostile, over-long input is truncated to the newest `CAP` items.

use std::collections::VecDeque;
use std::fmt;
use std::marker::PhantomData;

use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Bounded FIFO holding at most `CAP` items; pushing at capacity drops the oldest.
#[derive(Clone, Debug, PartialEq)]
pub struct Ring<T, const CAP: usize> {
    pub(crate) items: VecDeque<T>,
}

impl<T, const CAP: usize> Default for Ring<T, CAP> {
    fn default() -> Self {
        Self { items: VecDeque::new() }
    }
}

impl<T, const CAP: usize> Ring<T, CAP> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append `item`; returns `true` when the oldest item was dropped to make room.
    pub fn push(&mut self, item: T) -> bool {
        self.items.push_back(item);
        let mut dropped = false;
        while self.items.len() > CAP {
            self.items.pop_front();
            dropped = true;
        }
        dropped
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        CAP
    }

    /// Oldest to newest.
    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, T> {
        self.items.iter()
    }

    #[must_use]
    pub fn last(&self) -> Option<&T> {
        self.items.back()
    }

    /// Oldest to newest, mutable.
    pub fn iter_mut(&mut self) -> std::collections::vec_deque::IterMut<'_, T> {
        self.items.iter_mut()
    }

    pub fn last_mut(&mut self) -> Option<&mut T> {
        self.items.back_mut()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

impl<'a, T, const CAP: usize> IntoIterator for &'a Ring<T, CAP> {
    type Item = &'a T;
    type IntoIter = std::collections::vec_deque::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl<T: Serialize, const CAP: usize> Serialize for Ring<T, CAP> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.items.iter())
    }
}

struct RingVisitor<T, const CAP: usize>(PhantomData<T>);

impl<'de, T: Deserialize<'de>, const CAP: usize> Visitor<'de> for RingVisitor<T, CAP> {
    type Value = Ring<T, CAP>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a sequence")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut ring = Ring::new();
        while let Some(item) = seq.next_element()? {
            ring.push(item);
        }
        Ok(ring)
    }
}

impl<'de, T: Deserialize<'de>, const CAP: usize> Deserialize<'de> for Ring<T, CAP> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_seq(RingVisitor(PhantomData))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn keeps_latest_in_order_and_reports_drops() {
        let mut r: Ring<u32, 500> = Ring::new();
        let mut first_drop = None;
        for i in 0..1000 {
            if r.push(i) && first_drop.is_none() {
                first_drop = Some(i);
            }
        }
        assert_eq!(first_drop, Some(500));
        assert_eq!(r.len(), 500);
        assert!(r.iter().copied().eq(500..1000));
    }

    #[test]
    fn deserialisation_truncates_to_newest() {
        let json = serde_json::to_string(&(0..30).collect::<Vec<u32>>()).unwrap();
        let r: Ring<u32, 10> = serde_json::from_str(&json).unwrap();
        assert!(r.iter().copied().eq(20..30));
    }
}
