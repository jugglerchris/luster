use gc_arena::{Collect, CollectionContext};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::default::Default;
use std::borrow::Borrow;
use std::mem;

fn do_hash<K: Hash+Eq>(k: &K) -> usize {
    let mut hasher = DefaultHasher::new();
    k.hash(&mut hasher);
    hasher.finish() as usize
}

#[derive(Clone, Debug)]
enum Bucket<K, V> {
    Empty,      // Empty
    Full(K, V), // An actual value
    Tombstone,  // Empty (but was once full)
}

impl<K, V> Default for Bucket<K, V> {
    fn default() -> Self { Bucket::Empty }
}

#[derive(Clone, Debug)]
struct BucketStore<K, V> {
    buckets: Vec<Bucket<K, V>>,
    num_items: usize,
    capacity: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct HashMap<K, V> {
    buckets: BucketStore<K, V>,
}

const MIN_CAPACITY: usize = 8;

impl<K: Eq+Hash, V> BucketStore<K, V> {
    fn with_capacity(capacity: usize) -> Self {
        let mut buckets = Vec::new();
        buckets.resize_with(capacity, Default::default);
        let num_items = 0;
        BucketStore {
            buckets, num_items, capacity,
        }
    }
    pub fn insert(&mut self, k: K, v: V) -> Option<V>
    {
        assert!(self.num_items < self.capacity);
        let hash = do_hash(&k);
        let offset = hash % self.capacity;
        let mut idx = offset;
        loop {
            match self.buckets[idx] {
                Bucket::Empty |
                Bucket::Tombstone => {
                    // Found an empty bucket
                    self.buckets[idx] = Bucket::Full(k, v);
                    self.num_items += 1;
                    return None;
                }
                Bucket::Full(ref bk, ref mut bv) => {
                    if k == *bk {
                        return Some(mem::replace(bv, v));
                    }
                    // Else continue looking
                }
            }
            idx += 1;
            if idx >= self.capacity {
                idx = 0;
            }
            // If we get back to offset, then we must be full, but
            // we never allow that to happen.
            assert!(idx != offset);
        }
    }
}

impl<K: Eq+Hash, V> Default for HashMap<K, V> {
    fn default() -> Self {
        HashMap {
            buckets: BucketStore::with_capacity(MIN_CAPACITY),
        }
    }
}

pub struct HashMapIter<'a, K, V> {
    map: &'a BucketStore<K, V>,
    idx: usize,
}

impl<'a, K, V> Iterator for HashMapIter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        while self.idx < self.map.capacity {
            let idx = self.idx;
            self.idx += 1;
            match self.map.buckets[idx] {
                Bucket::Empty => (),
                Bucket::Tombstone => (),
                Bucket::Full(ref k, ref v) => { return Some((k, v)) }
            }
        }
        None
    }
}

impl<'a, K, V> IntoIterator for &'a BucketStore<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = HashMapIter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        HashMapIter {
            map: self,
            idx: 0,
        }
    }
}

struct BucketsIter<K, V> {
    buckets: BucketStore<K, V>,
}

impl<K, V> Iterator for BucketsIter<K, V> {
    type Item = (K, V);
    fn next(&mut self) -> Option< Self::Item> {
        loop {
            let bucket = self.buckets.buckets.pop();
            match bucket {
                None => return None,
                Some(Bucket::Empty) | Some(Bucket::Tombstone) => (), // Keep searching
                Some(Bucket::Full(k, v)) => {
                    return Some((k, v));
                }
            }
        }
    }
}

impl<K, V> IntoIterator for BucketStore<K, V> {
    type Item = (K, V);
    type IntoIter = BucketsIter<K, V>;
    fn into_iter(self) -> Self::IntoIter {
        BucketsIter {
            buckets: self,
        }
    }
}

impl<'a, K, V> IntoIterator for &'a HashMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = HashMapIter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        (&self.buckets).into_iter()
    }
}

unsafe impl<K, V> Collect for HashMap<K, V>
where
    K: Eq + Hash + Collect,
    V: Collect,
{
    #[inline]
    fn needs_trace() -> bool {
        K::needs_trace() || V::needs_trace()
    }

    #[inline]
    fn trace(&self, cc: CollectionContext) {
        for (k, v) in self {
            k.trace(cc);
            v.trace(cc);
        }
    }
}

impl<K: Eq + Hash, V> HashMap<K, V> {
    fn do_resize(&mut self, capacity: usize) {
        let new_buckets = BucketStore::with_capacity(capacity);
        let mut old_buckets = mem::replace(&mut self.buckets, new_buckets);

        for (k, v) in old_buckets.into_iter() {
            self.buckets.insert(k, v);
        }
    }
}

impl<K: Eq + Hash, V> HashMap<K, V> {
    pub fn get<Q: ?Sized>(&self, k: &Q) -> Option<&V>
    where K: Borrow<Q>,
          Q: Hash + Eq
    {
        unimplemented!()
    }

    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> bool
    where K: Borrow<Q>,
          Q: Hash + Eq
    {
        unimplemented!()
    }

    pub fn remove<Q: ?Sized>(&mut self, k: &Q) -> Option<V>
    where K: Borrow<Q>,
          Q: Hash + Eq
    {
        unimplemented!()
    }

    pub fn len(&self) -> usize {
        self.buckets.num_items
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn capacity(&self) -> usize {
        self.buckets.capacity
    }
    pub fn insert(&mut self, k: K, v: V) -> Option<V>
    {
        let hash = do_hash(&k);
        if self.buckets.num_items * 4 >= self.buckets.capacity * 3 {
            // three quarters full
            self.do_resize(self.buckets.capacity * 2);
        }
        self.buckets.insert(k, v)
    }

    pub fn keys(&self) -> impl Iterator<Item=&K> {
        (&self.buckets).into_iter()
                       .map(|(k, _)| k)
    }
    pub fn retain<F>(&mut self, f: F) where
        F: FnMut(&K, &mut V) -> bool
    {
        unimplemented!()
    }
    pub fn reserve(&mut self, additional: usize)
    {
        // TBD
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn make_hashmap() {
        let _map = HashMap::<isize, isize>::default();
    }

    #[test]
    fn insert_one() {
        let mut map = HashMap::default();
        map.insert(1isize, 4isize);
    }

    #[test]
    fn iterate() {
        let mut map = HashMap::default();
        map.insert(1isize, 4isize);
        map.insert(2, 3);
        let mut items: Vec<_> = map.into_iter().collect();
        items.sort();
        assert_eq!(&items[..], &[(&1, &4), (&2, &3)]);
    }

    #[test]
    fn resize() {
        let mut map = HashMap::default();
        for i in 0..50 {
            map.insert(i, i*i);
        }
        let mut items: Vec<_> = map.into_iter().collect();
        items.sort();
        assert_eq!(items.len(), 50);
    }
}