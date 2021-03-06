//! This crate handles the peer-to-peer networking part, currently with [`libp2p`].
//!
//! ## Procedure when adding events or new messages
//!
//! See the documentation of module [`balthazar`].
//!
//! ## Set up instructions
//!
//! TODO: base instructions to set it up.
#![allow(clippy::type_complexity)]

extern crate balthamisc as misc;
extern crate balthaproto as proto;
extern crate futures;
extern crate libp2p;
extern crate tokio;
extern crate tokio_util;
extern crate void;

use futures::{stream, Stream, StreamExt};
use libp2p::build_tcp_ws_noise_mplex_yamux;
/// To avoid importing the whole libp2p crate in another one...
pub use libp2p::{identity, Multiaddr};
use libp2p::{identity::Keypair, swarm::Swarm};
use misc::WorkerSpecs;
use std::task::{Context, Poll};

pub mod balthazar;
mod config;
pub mod tcp_transport;
mod wrapper;
pub use balthazar::{
    handler::{EventIn as HandlerIn, EventOut as HandlerOut},
    EventIn, EventOut, PeerRc,
};
pub use config::*;
pub use wrapper::{BalthBehavioursWrapper, InputHandle};

// TODO: Better interface with wrapper object
// TODO: NodeType containing manager to try ?
/// Creates a new swarm based on [`BalthBehaviour`](`balthazar::BalthBehaviour`) and a default transport and returns
/// a stream of event coming out of [`BalthBehaviour`](`balthazar::BalthBehaviour`).
pub async fn get_swarm<'a>(
    keypair: Keypair,
    config: &'a NetConfig,
    worker_specs: Option<&'a WorkerSpecs>,
) -> (InputHandle, impl Stream<Item = balthazar::EventOut>) {
    let keypair_public = keypair.public();
    let peer_id = keypair_public.into_peer_id();
    let (net_behaviour, tx) = BalthBehavioursWrapper::new(
        config.node_type_configuration().clone().map_worker(|w| {
            if let Some(specs) = worker_specs {
               (w, specs.clone())
            } else {
                panic!("No worker specs were provided, but the NetConfig provided contains worker config info.");
            }
        }),
        *config.manager_check_interval(),
        *config.manager_timeout(),
        &keypair,
    ).await;

    let transport = build_tcp_ws_noise_mplex_yamux(keypair).unwrap();

    let mut swarm = Swarm::new(transport, net_behaviour, peer_id);

    if let Some(addr) = config.listen_addr() {
        Swarm::listen_on(&mut swarm, addr.clone()).unwrap();
    }

    config.bootstrap_peers().iter().for_each(|addr| {
        Swarm::dial_addr(&mut swarm, addr.clone()).unwrap();
        println!("Dialed {:?}", addr);
    });

    for addr in Swarm::listeners(&swarm) {
        println!("Listening on {}", addr);
    }

    let mut listening = false;
    (
        tx,
        // TODO: use more general events: https://docs.rs/libp2p/0.15.0/libp2p/swarm/enum.SwarmEvent.html
        // TODO: not very clean... or is it ? (taken roughly from the examples)
        stream::poll_fn(move |cx: &mut Context| {
            let poll = swarm.poll_next_unpin(cx);
            if let Poll::Pending = poll {
                if !listening {
                    for addr in Swarm::listeners(&swarm) {
                        println!("Listening on {}", addr);
                        listening = true;
                    }
                }
            }

            poll
        }),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
