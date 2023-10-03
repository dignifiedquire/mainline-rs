use std::time::Duration;

use rand::RngCore;
use records::Records;
use rpc::Rpc;
use tables::Tables;
use url::Url;
use values::Values;

mod records;
mod rpc;
mod tables;
mod values;

/// Rotate secrets every 5 minutes
const ROTATE_INTERVAL: Duration = Duration::from_secs(5 * 60);

// TODO: make Hash generic and configurable, for now just use sha1
const HASH_LENGTH: usize = 20;

pub struct Dht {
    tables: Tables,
    values: Values,
    peers: Records,
    rpc: Rpc,
    secrets: Secrets,
    host: Option<Url>,
    listening: bool,
    destroyed: bool,
    node_id: [u8; 20],
    bucket_outdated_time_span: Duration,
}

pub struct Opts {
    /// 160-bit DHT node ID (default: randomly generated)
    pub node_id: Option<[u8; 20]>,
    /// Bootstrap servers (default: router.bittorrent.com:6881, router.utorrent.com:6881, dht.transmissionbt.com:6881)
    pub bootstrap: Vec<Url>,
    /// Host of local peer, if specified then announces get added to local table (disabled by default)
    pub host: Option<Url>,
    /// k-rpc option to specify maximum concurrent UDP requests allowed.
    pub concurrency: usize,
    /// Check buckets
    pub time_bucket_outdated: Duration,
    pub max_tables: usize,
    pub max_values: usize,
    /// Optional setting for announced peers to time out.
    pub max_age: Option<Duration>,
    pub max_peers: usize,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            node_id: None,
            bootstrap: vec![],
            host: None,
            concurrency: 16,
            time_bucket_outdated: Duration::from_secs(15 * 16),
            max_tables: 1000,
            max_values: 1000,
            max_age: None,
            max_peers: 10000,
        }
    }
}

impl Dht {
    pub fn new<R: RngCore>(opts: Opts, rng: &mut R) -> Self {
        let rpc = Rpc::new();
        // TODO: register "callbacks" to rpc
        // TODO: integrate verify "callback" (probably a trait)

        // TODO: setup interval to trigger secret rotation
        // every ROTATE_INTERVAL

        let node_id = opts.node_id.unwrap_or_else(|| {
            let mut bytes = [0u8; 20];
            rng.fill_bytes(&mut bytes);
            bytes
        });

        Dht {
            tables: Tables::new(ROTATE_INTERVAL, opts.max_tables),
            values: Values::new(opts.max_values),
            peers: Records::new(opts.max_age, opts.max_peers),
            secrets: Secrets::new(rng),
            rpc,
            host: opts.host,
            listening: false,
            destroyed: false,
            node_id,
            bucket_outdated_time_span: opts.time_bucket_outdated,
        }
    }
}

struct Secrets {
    a: [u8; HASH_LENGTH],
    b: [u8; HASH_LENGTH],
}

impl Secrets {
    fn new<R: RngCore>(rng: &mut R) -> Self {
        let mut s = Secrets {
            a: [0u8; HASH_LENGTH],
            b: [0u8; HASH_LENGTH],
        };
        rng.fill_bytes(&mut s.a);
        rng.fill_bytes(&mut s.b);
        s
    }

    fn rotate<R: RngCore>(&mut self, rng: &mut R) {
        std::mem::swap(&mut self.a, &mut self.b);
        rng.fill_bytes(&mut self.a);
    }
}
