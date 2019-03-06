use std::error::Error as StdError;
use std::hash::{Hash, Hasher};
use std::{fmt, i64, mem};

use num_traits::cast;
//use rustc_hash::FxHashMap;

use gc_arena::{Collect, GcCell, MutationContext};

use crate::Value;
use crate::hashmap::HashMap;
type HM<K, V> = HashMap<K, V>;

#[derive(Debug, Copy, Clone, Collect)]
#[collect(require_copy)]
pub struct Table<'gc>(pub GcCell<'gc, TableState<'gc>>);

#[derive(Debug, Clone, Copy, Collect)]
#[collect(require_static)]
pub enum InvalidTableKey {
    IsNaN,
    IsNil,
}

impl StdError for InvalidTableKey {}

impl fmt::Display for InvalidTableKey {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            InvalidTableKey::IsNaN => write!(fmt, "table key is NaN"),
            InvalidTableKey::IsNil => write!(fmt, "table key is Nil"),
        }
    }
}

impl<'gc> PartialEq for Table<'gc> {
    fn eq(&self, other: &Table<'gc>) -> bool {
        GcCell::ptr_eq(self.0, other.0)
    }
}

impl<'gc> Eq for Table<'gc> {}

impl<'gc> Hash for Table<'gc> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_ptr().hash(state);
    }
}

impl<'gc> Table<'gc> {
    pub fn new(mc: MutationContext<'gc, '_>) -> Table<'gc> {
        Table(GcCell::allocate(mc, TableState::default()))
    }

    pub fn get<K: Into<Value<'gc>>>(&self, key: K) -> Value<'gc> {
        self.0.read().get(key.into())
    }

    pub fn set<K: Into<Value<'gc>>, V: Into<Value<'gc>>>(
        &self,
        mc: MutationContext<'gc, '_>,
        key: K,
        value: V,
    ) -> Result<Value<'gc>, InvalidTableKey> {
        self.0.write(mc).set(key.into(), value.into())
    }

    pub fn length(&self) -> i64 {
        self.0.read().length()
    }
}

#[derive(Debug, Collect, Default)]
#[collect(empty_drop)]
pub struct TableState<'gc> {
    array: Vec<Value<'gc>>,
    map: HM<TableKey<'gc>, Value<'gc>>,
}

impl<'gc> TableState<'gc> {
    pub fn get(&self, key: Value<'gc>) -> Value<'gc> {
        if let Some(index) = to_array_index(key) {
            if index < self.array.len() {
                return self.array[index];
            }
        }

        if let Ok(key) = TableKey::new(key) {
            self.map.get(&key).cloned().unwrap_or(Value::Nil)
        } else {
            Value::Nil
        }
    }

    pub fn set(
        &mut self,
        key: Value<'gc>,
        value: Value<'gc>,
    ) -> Result<Value<'gc>, InvalidTableKey> {
        let index_key = to_array_index(key);
        if let Some(index) = index_key {
            if index < self.array.len() {
                return Ok(mem::replace(&mut self.array[index], value));
            }
        }

        let hash_key = TableKey::new(key)?;
        if value == Value::Nil {
            Ok(self.map.remove(&hash_key).unwrap_or(Value::Nil))
        } else if self.map.len() < self.map.capacity() {
            Ok(self.map.insert(hash_key, value).unwrap_or(Value::Nil))
        } else {
            // If a new element does not fit in either the array or map part of the table, we need
            // to grow.  First, we find the total count of array candidate elements across the array
            // part, the map part, and the newly inserted key.

            const USIZE_BITS: usize = mem::size_of::<usize>() * 8;

            // Count of array-candidate elements based on the highest bit in the index
            let mut array_counts = [0; USIZE_BITS];
            // Total count of all array-candidate elements
            let mut array_total = 0;

            for (i, e) in self.array.iter().enumerate() {
                if *e != Value::Nil {
                    array_counts[highest_bit(i)] += 1;
                    array_total += 1;
                }
            }

            for k in self.map.keys() {
                if let Some(i) = to_array_index(k.0) {
                    array_counts[highest_bit(i)] += 1;
                    array_total += 1;
                }
            }

            if let Some(i) = index_key {
                array_counts[highest_bit(i)] += 1;
                array_total += 1;
            }

            // Then, we compute the new optimal size for the array by finding the largest array size
            // such that at least half of the elements in the array would be in use.

            let mut optimal_size = 0;
            let mut total = 0;
            for i in 0..USIZE_BITS {
                if (1 << i) / 2 >= array_total {
                    break;
                }

                if array_counts[i] > 0 {
                    total += array_counts[i];
                    if total > (1 << i) / 2 {
                        optimal_size = 1 << i;
                    }
                }
            }

            let old_array_size = self.array.len();
            let old_map_size = self.map.len();
            if optimal_size > old_array_size {
                // If we're growing the array part, we need to grow the array and take any newly valid
                // array keys from the map part.

                self.array.reserve(optimal_size - old_array_size);
                let capacity = self.array.capacity();
                self.array.resize(capacity, Value::Nil);

                let array = &mut self.array;
                self.map.retain(|k, v| {
                    if let Some(i) = to_array_index(k.0) {
                        if i < array.len() {
                            array[i] = *v;
                            return false;
                        }
                    }
                    true
                });
            } else {
                // If we aren't growing the array, we're adding a new element to the map that won't
                // fit in the advertised capacity.  The capacity of std::collections::HashMap is
                // just a lower-bound, so we may actually be able to insert past the capacity
                // without the advertised capacity growing, so to make sure that we don't try to
                // grow repeatedly, we need to make sure the capacity actually increases.  We simply
                // double the capacity here.
                self.map.reserve(old_map_size);
            }

            // Now we can insert the new key value pair
            if let Some(index) = index_key {
                if index < self.array.len() {
                    return Ok(mem::replace(&mut self.array[index], value));
                }
            }
            Ok(self.map.insert(hash_key, value).unwrap_or(Value::Nil))
        }
    }

    /// Returns a 'border' for this table.
    ///
    /// A 'border' for a table is any i >= 0 where:
    /// `(i == 0 or table[i] ~= nil) and table[i + 1] == nil`
    ///
    /// If a table has exactly one border, it is called a 'sequence', and this border is the table's
    /// length.
    pub fn length(&self) -> i64 {
        // Binary search for a border.  Entry at max must be Nil, min must be 0 or entry at min must
        // be != Nil.
        fn binary_search<F: Fn(i64) -> bool>(mut min: i64, mut max: i64, is_nil: F) -> i64 {
            while max - min > 1 {
                let mid = min + (max - min) / 2;
                if is_nil(mid) {
                    max = mid;
                } else {
                    min = mid;
                }
            }
            min
        }

        let array_len: i64 = cast(self.array.len()).unwrap();

        if !self.array.is_empty() && self.array[array_len as usize - 1] == Value::Nil {
            // If the array part ends in a Nil, there must be a border inside it
            binary_search(0, array_len, |i| self.array[i as usize - 1] == Value::Nil)
        } else if self.map.is_empty() {
            // If there is no border in the arraay but the map part is empty, then the array length
            // is a border
            array_len
        } else {
            // Otherwise, we must check the map part for a border.  We need to find some nil value
            // in the map part as the max for a binary search.
            let min = array_len;
            let mut max = array_len.checked_add(1).unwrap();
            while self.map.contains_key(&TableKey(Value::Integer(max))) {
                if max == i64::MAX {
                    // If we can't find a nil entry by doubling, then the table is pathalogical.  We
                    // return the favor with a pathalogical answer: i64::MAX + 1 can't exist in the
                    // table, therefore it is Nil, so since the table contains i64::MAX, i64::MAX is
                    // a border.
                    return i64::MAX;
                } else if let Some(double_max) = max.checked_mul(2) {
                    max = double_max;
                } else {
                    max = i64::MAX;
                }
            }

            // We have found a max where table[max] == nil, so we can now binary search
            binary_search(min, max, |i| {
                !self.map.contains_key(&TableKey(Value::Integer(i)))
            })
        }
    }
}

// Value which implements Hash and Eq, and cannot contain Nil or NaN values.
#[derive(Debug, Collect, PartialEq)]
#[collect(empty_drop)]
struct TableKey<'gc>(Value<'gc>);

impl<'gc> Eq for TableKey<'gc> {}

impl<'gc> Hash for TableKey<'gc> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.0 {
            Value::Nil => unreachable!(),
            Value::Boolean(b) => {
                Hash::hash(&1, state);
                b.hash(state);
            }
            Value::Integer(i) => {
                Hash::hash(&2, state);
                i.hash(state);
            }
            Value::Number(n) => {
                Hash::hash(&3, state);
                canonical_float_bytes(*n).hash(state);
            }
            Value::String(s) => {
                Hash::hash(&4, state);
                s.hash(state);
            }
            Value::Table(t) => {
                Hash::hash(&5, state);
                t.hash(state);
            }
            Value::Function(c) => {
                Hash::hash(&6, state);
                c.hash(state);
            }
            Value::Thread(t) => {
                Hash::hash(&7, state);
                t.hash(state);
            }
        }
    }
}

impl<'gc> TableKey<'gc> {
    fn new(value: Value<'gc>) -> Result<TableKey<'gc>, InvalidTableKey> {
        match value {
            Value::Nil => Err(InvalidTableKey::IsNil),
            Value::Number(n) => {
                // NaN keys are disallowed, f64 keys where their closest i64 representation is equal
                // to themselves when cast back to f64 are considered integer keys.
                if n.is_nan() {
                    Err(InvalidTableKey::IsNaN)
                } else if let Some(i) = f64_to_i64(n) {
                    Ok(TableKey(Value::Integer(i)))
                } else {
                    Ok(TableKey(Value::Number(n)))
                }
            }
            v => Ok(TableKey(v)),
        }
    }
}

// Returns the closest i64 to a given f64 such that casting the i64 back to an f64 results in an
// equal value, if such an integer exists.
fn f64_to_i64(n: f64) -> Option<i64> {
    let i = cast::<_, i64>(n)?;
    if i as f64 == n {
        Some(i)
    } else {
        None
    }
}

// Parameter must not be NaN, should return a bit-pattern which is always equal when the
// corresponding f64s are equal (-0.0 and 0.0 return the same bit pattern).
fn canonical_float_bytes(f: f64) -> u64 {
    assert!(!f.is_nan());
    unsafe {
        if f == 0.0 {
            mem::transmute(0.0f64)
        } else {
            mem::transmute(f)
        }
    }
}

// If the given key can live in the array part of the table (integral value between 1 and
// usize::MAX), returns the associated array index.
fn to_array_index<'gc>(key: Value<'gc>) -> Option<usize> {
    let i = match key {
        Value::Integer(i) => cast::<_, i64>(i)?,
        Value::Number(f) => {
            if let Some(i) = f64_to_i64(f) {
                i
            } else {
                return None;
            }
        }
        _ => {
            return None;
        }
    };

    if i > 0 {
        Some(i as usize - 1)
    } else {
        None
    }
}

// Returns the place of the highest set bit in the given i, i = 0 returns 0, i = 1 returns 1, i = 2
// returns 2, i = 3 returns 2, and so on.
fn highest_bit(mut i: usize) -> usize {
    const LOG_2: [u8; 256] = [
        0, 1, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
        5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
        6, 6, 6, 6, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
        7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
        7, 7, 7, 7, 7, 7, 7, 7, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
        8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
        8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
        8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
        8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    ];

    let mut hb = 0;
    while i >= 256 {
        hb += 8;
        i >>= 8;
    }

    hb + LOG_2[i] as usize
}
