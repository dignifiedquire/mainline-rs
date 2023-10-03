use std::time::Duration;

use anyhow::Result;
use lru::LruCache;
use url::Host;

use crate::{kbucket::Kbucket, HASH_LENGTH};

// new LRU({ maxAge: ROTATE_INTERVAL, max: opts.maxTables || 1000 })

type Key = [u8; HASH_LENGTH];

pub struct Contact {
    id: [u8; 20],
    host: Host,
    port: u16,
    token: Vec<u8>,
}

pub struct Tables(LruCache<Key, Kbucket<Contact>>);
impl Tables {
    pub fn new(max_age: Duration, max: usize) -> Result<Self> {
        // TODO: figure out max_age

        Ok(Tables(LruCache::new(max.try_into()?)))
    }
}
