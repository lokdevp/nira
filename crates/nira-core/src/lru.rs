//! A tiny LRU cache with a byte budget, used to keep decoded page textures
//! around so that revisiting nearby pages is instant.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

pub struct LruCache<K: Eq + Hash + Clone, V> {
    map: HashMap<K, (V, usize)>,
    order: VecDeque<K>,
    capacity: usize,
    max_bytes: usize,
    bytes: usize,
}

impl<K: Eq + Hash + Clone, V> LruCache<K, V> {
    pub fn new(capacity: usize, max_bytes: usize) -> Self {
        LruCache {
            map: HashMap::new(),
            order: VecDeque::new(),
            capacity: capacity.max(1),
            max_bytes,
            bytes: 0,
        }
    }

    pub fn contains(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    /// Returns a reference to the value, promoting the entry to
    /// most-recently-used.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.map.contains_key(key) {
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                let k = self.order.remove(pos).unwrap();
                self.order.push_back(k);
            }
            self.map.get(key).map(|(v, _)| v)
        } else {
            None
        }
    }

    /// Returns a reference to the value without touching the LRU order.
    pub fn peek(&self, key: &K) -> Option<&V> {
        self.map.get(key).map(|(v, _)| v)
    }

    /// Insert or update an entry (value + its approximate size in bytes).
    pub fn put(&mut self, key: K, value: V, bytes: usize) {
        if let Some(entry) = self.map.get_mut(&key) {
            self.bytes = self.bytes.saturating_sub(entry.1);
            entry.0 = value;
            entry.1 = bytes;
            self.bytes = self.bytes.saturating_add(bytes);
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                let k = self.order.remove(pos).unwrap();
                self.order.push_back(k);
            }
        } else {
            self.map.insert(key.clone(), (value, bytes));
            self.order.push_back(key);
            self.bytes = self.bytes.saturating_add(bytes);
        }
        self.evict();
    }

    /// Insert an entry, evicting only entries `keep` doesn't hold,
    /// least recently used first. Kept entries may leave the cache over
    /// its limits (the pages on screen must stay).
    pub fn put_keeping(&mut self, key: K, value: V, bytes: usize, keep: impl Fn(&K) -> bool) {
        self.remove(&key);
        self.map.insert(key.clone(), (value, bytes));
        self.order.push_back(key.clone());
        self.bytes = self.bytes.saturating_add(bytes);
        self.trim(|k| *k == key || keep(k));
    }

    /// Hold at most `capacity` entries from now on, evicting the least
    /// recently used ones `keep` doesn't hold if there are too many.
    pub fn set_capacity(&mut self, capacity: usize, keep: impl Fn(&K) -> bool) {
        self.capacity = capacity.max(1);
        self.trim(keep);
    }

    /// Evict entries `keep` doesn't hold, least recently used first, until
    /// the cache is within its limits (or only kept entries are left).
    fn trim(&mut self, keep: impl Fn(&K) -> bool) {
        let victims: Vec<K> = self.order.iter().filter(|k| !keep(k)).cloned().collect();
        for k in victims {
            if !self.over_limits() {
                break;
            }
            self.remove(&k);
        }
    }

    /// Insert an entry if evicting entries `may_evict` allows (least
    /// recently used first) makes room for it. Otherwise change nothing
    /// and return false.
    pub fn put_if_room(
        &mut self,
        key: K,
        value: V,
        bytes: usize,
        may_evict: impl Fn(&K) -> bool,
    ) -> bool {
        let old = self.map.get(&key).map(|(_, b)| *b);
        let count = self.map.len() + usize::from(old.is_none());
        let total = (self.bytes - old.unwrap_or(0)).saturating_add(bytes);
        let mut over_count = count.saturating_sub(self.capacity);
        let mut over_bytes = total.saturating_sub(self.max_bytes);
        let mut victims = Vec::new();
        for k in &self.order {
            if over_count == 0 && over_bytes == 0 {
                break;
            }
            if *k != key && may_evict(k) {
                over_count = over_count.saturating_sub(1);
                over_bytes = over_bytes.saturating_sub(self.map[k].1);
                victims.push(k.clone());
            }
        }
        if over_count > 0 || over_bytes > 0 {
            return false;
        }
        for k in victims {
            self.remove(&k);
        }
        self.remove(&key);
        self.map.insert(key.clone(), (value, bytes));
        self.order.push_back(key);
        self.bytes = self.bytes.saturating_add(bytes);
        true
    }

    fn over_limits(&self) -> bool {
        self.map.len() > self.capacity || self.bytes > self.max_bytes
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some((value, bytes)) = self.map.remove(key) {
            self.bytes = self.bytes.saturating_sub(bytes);
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
            Some(value)
        } else {
            None
        }
    }

    /// Drop entries whose key does not satisfy `f` (used to trim pages that
    /// fell out of the wanted window).
    pub fn retain(&mut self, mut f: impl FnMut(&K) -> bool) {
        let drop_keys: Vec<K> = self.order.iter().filter(|k| !f(k)).cloned().collect();
        for k in drop_keys {
            self.remove(&k);
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
        self.bytes = 0;
    }

    fn evict(&mut self) {
        while self.map.len() > self.capacity || (self.bytes > self.max_bytes && self.map.len() > 1)
        {
            let Some(key) = self.order.pop_front() else {
                break;
            };
            if let Some((_, bytes)) = self.map.remove(&key) {
                self.bytes = self.bytes.saturating_sub(bytes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_least_recently_used() {
        let mut c = LruCache::new(3, 1000);
        c.put(1, "a", 1);
        c.put(2, "b", 1);
        c.put(3, "c", 1);
        let _ = c.get(&1); // promote 1
        c.put(4, "d", 1); // evicts 2 (LRU)
        assert!(!c.contains(&2));
        assert!(c.contains(&1));
        assert!(c.contains(&3));
        assert!(c.contains(&4));
    }

    #[test]
    fn respects_byte_budget() {
        let mut c = LruCache::new(100, 10);
        c.put(1, "x", 6);
        c.put(2, "y", 6);
        // total 12 > 10 -> evicts one
        assert_eq!(c.map.len(), 1);
    }

    #[test]
    fn put_keeping_never_evicts_kept_entries() {
        let mut c = LruCache::new(10, 10);
        c.put(1, "shown", 6);
        c.put_keeping(2, "next", 6, |k| *k == 1);
        // Over budget, but the kept entry stays and the newcomer is in.
        assert!(c.contains(&1) && c.contains(&2));
        c.put_keeping(3, "more", 3, |k| *k == 1);
        assert!(c.contains(&1) && !c.contains(&2) && c.contains(&3));
    }

    #[test]
    fn set_capacity_keeps_entries_when_growing_and_evicts_lru_when_shrinking() {
        let mut c = LruCache::new(3, 1000);
        c.put(1, "a", 1);
        c.put(2, "b", 1);
        c.put(3, "c", 1);
        c.set_capacity(5, |_| false);
        assert!(c.contains(&1) && c.contains(&2) && c.contains(&3));
        c.put(4, "d", 1);
        assert_eq!(c.map.len(), 4, "the larger capacity is used");
        let _ = c.get(&1);
        // Shrinking evicts the least recently used, but never a kept one.
        c.set_capacity(2, |k| *k == 2);
        assert!(c.contains(&2) && c.contains(&1));
        assert!(!c.contains(&3) && !c.contains(&4));
    }

    #[test]
    fn put_if_room_only_evicts_what_it_may() {
        let mut c = LruCache::new(10, 10);
        c.put(1, "a", 4);
        c.put(2, "b", 4);
        // Needs 2 bytes freed; only entry 2 may go.
        assert!(c.put_if_room(3, "c", 4, |k| *k == 2));
        assert!(c.contains(&1) && !c.contains(&2) && c.contains(&3));
        // No room and nothing may go: unchanged.
        assert!(!c.put_if_room(4, "d", 4, |_| false));
        assert!(!c.contains(&4) && c.bytes == 8);
        // The count limit counts too.
        let mut c = LruCache::new(1, 100);
        c.put(1, "a", 1);
        assert!(!c.put_if_room(2, "b", 1, |_| false));
        assert!(c.put_if_room(2, "b", 1, |_| true));
        assert!(!c.contains(&1) && c.contains(&2));
    }

    #[test]
    fn retain_drops_only_unwanted() {
        let mut c = LruCache::new(10, 1000);
        c.put(1, "a", 1);
        c.put(2, "b", 1);
        c.put(3, "c", 1);
        c.retain(|k| *k != 2);
        assert!(!c.contains(&2));
        assert!(c.contains(&1) && c.contains(&3));
    }
}
