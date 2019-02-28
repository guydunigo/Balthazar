use tokio::codec::FramedRead;
use tokio::io;
use tokio::net::TcpListener;
use tokio::prelude::*;

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::*;
// TODO: local Error
use super::Error;
use balthmessage::Proto;

/// **Important** : peer must be a reference to self.
fn handle_vote(
    mpsc_tx: mpsc::Sender<Proto>,
    peer_locked: &mut Peer,
    peer: PeerArcMut,
    local_vote: ConnVote,
    peer_vote: ConnVote,
) -> Result<(), Error> {
    if local_vote < peer_vote {
        println!("Listener : Vote : peer won, cancelling connection...");
        peer_locked.listener_connection_cancel(mpsc_tx);
        return Err(Error::ConnectionCancelled);
    } else if local_vote > peer_vote {
        println!("Listener : Vote : peer lost, validating connection...");
        peer_locked.listener_connection_ack(peer, mpsc_tx);
    } else {
        println!("Listener : Vote : Equality, cancelling connection...");
        // TODO: cancelling was the easiest but is there a better way ?
        peer_locked.listener_connection_cancel(mpsc_tx);
    }

    Ok(())
}

// TODO: take care of ConnectCancel from client ?
// TODO: handle packet in EVERY branches ?
fn for_each_packet_connecting(
    shoal: ShoalReadArc,
    peer_opt: PeerArcMutOpt,
    peer_addr: SocketAddr,
    mpsc_tx: mpsc::Sender<Proto>,
    pkt: Proto,
) -> Result<(), Error> {
    let mut peer_opt = peer_opt.lock().unwrap();
    if let Some(ref peer) = *peer_opt {
        // println!("Listener : `peer_opt` is `Some()`.");

        let mut peer_locked = peer.lock().unwrap();

        // println!("Listener : `peer.state` is `{:?}`.", peer_locked.state);
        match peer_locked.state {
            PeerState::Connecting(local_vote) => {
                if peer_locked.client_connecting {
                    match pkt {
                        Proto::Vote(peer_vote) => handle_vote(
                            mpsc_tx,
                            &mut *peer_locked,
                            peer.clone(),
                            local_vote,
                            peer_vote,
                        )?,
                        _ => eprintln!("Listener : received a packet but it was not `Vote(vote)`."),
                    }
                } else {
                    peer_locked.listener_connection_ack(peer.clone(), mpsc_tx);
                    // End the packet listening loop :
                    return Err(Error::ConnectionEnded);
                }
            }
            PeerState::Connected(_) => {
                println!("Listener : `peer.state` is `Connected`, stopping connection loop.");

                // TODO: keep the same receiving frame and just transfer some channel or so...
                peer_locked
                    .handle_pkt(pkt)
                    .expect(&format!("Client : {} : Error forwarding pkt...", peer_addr)[..]);

                // End the packet listening loop :
                return Err(Error::ConnectionEnded);
            }
            _ => eprintln!(
                "Listener : `peer.state` shouldn't be `{:?}` when `peer_opt` is `Some(peer)`.",
                peer_locked.state
            ),
        }
    } else {
        // println!("Listener : `peer_opt` is `None`.");
        match pkt {
            Proto::Connect(peer_pid) => {
                let peers = shoal.lock().peers();
                let mut peers_locked = peers.lock().unwrap();

                if let Some(peer_from_peers) = peers_locked.get(&peer_pid) {
                    // println!("Listener : {} : Peer is in peers.", peer_addr);
                    *peer_opt = Some(peer_from_peers.clone());

                    let mut peer = peer_from_peers.lock().unwrap();

                    // println!("Listener : `peer.state` is `{:?}`.", peer.state);
                    match peer.state {
                        PeerState::NotConnected => {
                            if peer.pid() != peer_pid {
                                eprintln!("Client : {} : Received a `peer_id` that differs from the already known `peer.pid` : `peer_id=={}`, `peer.peer_id=={}`.", peer.addr, peer_pid, peer.pid());
                                // TODO: return error and cancel connection ?
                            }

                            peer.listener_connection_ack(peer_from_peers.clone(), mpsc_tx);
                            // End the packet listening loop :
                            return Err(Error::ConnectionEnded);
                        }
                        PeerState::Connected(_) => {
                            // eprintln!("Listener : Someone tried to connect with pid `{}` but it is already connected (`state` is `Connected`). Cancelling...", peer_pid);
                            cancel_connection(mpsc_tx);
                            // End the packet listening loop :
                            return Err(Error::ConnectionCancelled);
                        }
                        PeerState::Connecting(_local_vote) => {
                            if peer.listener_connecting {
                                // eprintln!("Listener : Someone tried to connect with pid `{}` but it is in connection with a listener (`state` is `Connected` and `listener_connecting` is `true`). Cancelling...", peer_pid);
                                cancel_connection(mpsc_tx);
                                return Err(Error::ConnectionCancelled);
                            } else if !peer.client_connecting {
                                panic!("Listener : Peer inconsistency : `state` is `Connecting` but `listener_connecting` and `client_connecting` are both false.");
                            }
                        }
                    }
                } else {
                    // println!("Listener : {} : Peer is not in peers.", peer_addr);

                    let peer = Peer::new(shoal.clone(), peer_pid, peer_addr);
                    let peer_arc_mut = Arc::new(Mutex::new(peer));

                    {
                        let mut peer = peer_arc_mut.lock().unwrap();
                        peer.listener_connection_ack(peer_arc_mut.clone(), mpsc_tx);
                    }

                    *peer_opt = Some(peer_arc_mut.clone());
                    shoal.lock().insert_peer(&mut peers_locked, peer_arc_mut);

                    return Err(Error::ConnectionEnded);
                }
            }
            _ => eprintln!("Listener : received a packet but it was not `Connect(pid,vote)`."),
        }
    }
    Ok(())
}

pub fn bind(local_addr: &SocketAddr) -> Result<TcpListener, io::Error> {
    TcpListener::bind(local_addr)
}

pub fn listen(shoal: ShoalReadArc, listener: TcpListener) -> impl Future<Item = (), Error = ()> {
    listener
        .incoming()
        .for_each(move |socket| {
            let peer_addr = socket.peer_addr()?;

            let (rx, tx) = socket.split();
            let mpsc_tx = write_to_mpsc(tx);

            let local_pid = shoal.lock().local_pid;

            let shoal = shoal.clone();
            println!("Listener : Asked for connection : `{}`", peer_addr);

            let peer_opt = Arc::new(Mutex::new(None));

            let send_future = send_packet(mpsc_tx.clone(), Proto::Connect(local_pid))
                .map_err(|_| ())
                .and_then(move |_| {
                    let framed_sock = FramedRead::new(rx, ProtoCodec::new(None));

                    let connecting = AtomicBool::new(true);

                    framed_sock
                        .map_err(Error::from)
                        .for_each(move |pkt| {
                            // TODO: there might still be inconstistencies if two messages are
                            // received at same time, use mutex ?
                            if connecting.load(Ordering::Acquire) {
                                let res = for_each_packet_connecting(
                                    shoal.clone(),
                                    peer_opt.clone(),
                                    peer_addr,
                                    mpsc_tx.clone(),
                                    pkt,
                                );

                                if let Err(Error::ConnectionEnded) = res {
                                    connecting.store(false, Ordering::AcqRel);
                                    Ok(())
                                } else {
                                    res
                                }
                            } else {
                                let peer_opt = peer_opt.lock().unwrap();
                                if let Some(ref peer) = *peer_opt {
                                    let mut peer_locked = peer.lock().unwrap();

                                    peer_locked.handle_pkt(pkt)
                                } else {
                                    panic!("Can't be 'connected' and peer_opt=None !");
                                }
                            }
                        })
                        .map_err(move |err| match err {
                            // TODO: println anyway ?
                            Error::ConnectionCancelled | Error::ConnectionEnded => (),
                            _ => eprintln!("Listener : error when receiving a packet : {:?}.", err),
                        })
                });

            tokio::spawn(send_future);

            Ok(())
        })
        .map_err(|err| eprintln!("Listener : {:?}", err))
}
