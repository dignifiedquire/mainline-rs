use anyhow::Result;
use lru::LruCache;

use crate::HASH_LENGTH;

type Key = [u8; HASH_LENGTH];

pub struct Values(LruCache<Key, Value>);

pub struct Value {
    id: [u8; 20],
    token: Vec<u8>,
    v: Vec<u8>,
}

impl Values {
    pub fn new(max: usize) -> Result<Self> {
        Ok(Values(LruCache::new(max.try_into()?)))
    }
}
