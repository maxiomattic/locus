//! An in-process transport, for tests and for a client and server sharing a
//! machine. It can be switched off, or made to lose replies, like a network.

use crate::protocol::{Request, Response, Transport, Unreachable};
use crate::server::HomeServer;
use locus_model::Principal;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Controls the pretend network between one client and its server.
#[derive(Clone, Default)]
pub struct Network {
    offline: Arc<AtomicBool>,
    lose_replies: Arc<AtomicUsize>,
}

impl Network {
    pub fn set_online(&self, online: bool) {
        self.offline.store(!online, Ordering::SeqCst);
    }

    /// The server handles the next `n` requests, but the replies are lost.
    pub fn lose_next_replies(&self, n: usize) {
        self.lose_replies.store(n, Ordering::SeqCst);
    }
}

pub struct LocalTransport {
    server: HomeServer,
    who: Principal,
    net: Network,
}

impl LocalTransport {
    pub fn new(server: HomeServer, who: Principal, net: Network) -> Self {
        LocalTransport { server, who, net }
    }
}

impl Transport for LocalTransport {
    fn call(&self, req: Request) -> Result<Response, Unreachable> {
        if self.net.offline.load(Ordering::SeqCst) {
            return Err(Unreachable("the network is off".to_string()));
        }
        let reply = self.server.handle(&self.who, req);
        let lost = self.net.lose_replies.load(Ordering::SeqCst);
        if lost > 0 {
            self.net.lose_replies.store(lost - 1, Ordering::SeqCst);
            return Err(Unreachable("the reply was lost".to_string()));
        }
        Ok(reply)
    }
}
