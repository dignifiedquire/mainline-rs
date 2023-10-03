use std::time::Duration;

use anyhow::Result;
use rand::RngCore;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use url::Url;

use self::records::Records;
use self::rpc::Rpc;
use self::tables::Tables;
use self::values::Values;

mod kbucket;
mod records;
mod rpc;
mod tables;
mod values;

/// Rotate secrets every 5 minutes
const ROTATE_INTERVAL: Duration = Duration::from_secs(5 * 60);

// TODO: make Hash generic and configurable, for now just use sha1
pub(crate) const HASH_LENGTH: usize = 20;

pub struct Dht {
    actor_sender: mpsc::Sender<ActorMessage>,
    actor_handle: JoinHandle<()>,
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
    pub async fn new<R: RngCore + Send + 'static>(opts: Opts, rng: R) -> Result<Self> {
        let (actor_sender, actor_receiver) = mpsc::channel(64);

        let actor = Actor::new(opts, rng)?;

        let actor_handle = tokio::task::spawn(async move {
            actor.run(actor_receiver).await;
        });

        Ok(Dht {
            actor_sender,
            actor_handle,
        })
    }

    pub async fn shutdown(self) -> Result<()> {
        self.actor_sender.send(ActorMessage::Shutdown).await.ok();
        self.actor_handle.await?;
        Ok(())
    }
}

enum ActorMessage {
    Shutdown,
}

struct Actor {
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
    rng: Box<dyn RngCore + Send + 'static>,
}

impl Actor {
    fn new<R: RngCore + Send + 'static>(opts: Opts, mut rng: R) -> Result<Self> {
        let rpc = Rpc::new();
        // TODO: register "callbacks" to rpc
        // TODO: integrate verify "callback" (probably a trait)

        let node_id = opts.node_id.unwrap_or_else(|| {
            let mut bytes = [0u8; 20];
            rng.fill_bytes(&mut bytes);
            bytes
        });

        Ok(Actor {
            tables: Tables::new(ROTATE_INTERVAL, opts.max_tables)?,
            values: Values::new(opts.max_values)?,
            peers: Records::new(opts.max_age, opts.max_peers),
            secrets: Secrets::new(&mut rng),
            rpc,
            host: opts.host,
            listening: false,
            destroyed: false,
            node_id,
            bucket_outdated_time_span: opts.time_bucket_outdated,
            rng: Box::new(rng),
        })
    }

    async fn run(mut self, mut actor_receiver: mpsc::Receiver<ActorMessage>) {
        // Setup interval to trigger secret rotation
        let mut interval = tokio::time::interval_at(
            tokio::time::Instant::now() + ROTATE_INTERVAL,
            ROTATE_INTERVAL,
        );

        loop {
            tokio::select! {
                biased;

                Some(msg) = actor_receiver.recv() => {
                    match msg {
                        ActorMessage::Shutdown => {
                            break;
                        }
                    }
                }
                _ = interval.tick() => {
                    self.secrets.rotate(&mut self.rng);
                }
                else => {
                    break;
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_startup() {
        let rng = rand::rngs::OsRng::default();
        let dht = Dht::new(Opts::default(), rng).await.unwrap();
        dht.shutdown().await.unwrap();
    }
}
