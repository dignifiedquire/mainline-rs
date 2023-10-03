use std::time::Duration;

// records({
//       maxAge: opts.maxAge || 0,
//       maxSize: opts.maxPeers || 10000
//     })
pub struct Records {}

impl Records {
    pub fn new(max_age: Option<Duration>, max_peers: usize) -> Self {
        Records {}
    }
}
