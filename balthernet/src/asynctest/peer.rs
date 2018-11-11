use rand::random;
use tokio::codec::Framed;
use tokio::net::TcpStream;
use tokio::prelude::*;

use std::collections::HashMap;
use std::net::{Shutdown, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::{Error, MessageCodec};
use balthmessage::Message;

pub type Pid = u32;
pub type ConnVote = u32;

// TODO: beware of deadlocking a peer ?
// TODO: async lock?
pub type PeerArcMut = Arc<Mutex<Peer>>;
pub type PeersMapArcMut = Arc<Mutex<HashMap<Pid, PeerArcMut>>>;
pub type PeerArcMutOpt = Arc<Mutex<Option<PeerArcMut>>>;

fn vote() -> ConnVote {
    random()
}

/// Don't forget to spawn that...
pub fn send_message(
    socket: TcpStream,
    msg: Message,
) -> impl Future<Item = Framed<TcpStream, MessageCodec>, Error = Error> {
    // TODO: unwrap?
    let framed_sock = Framed::new(socket, MessageCodec::new());

    framed_sock.send(msg).map_err(move |err| {
        eprintln!("Error when sending message : `{:?}`.", err);
        Error::from(err)
    })
}

pub fn send_message_and_spawn(socket: TcpStream, msg: Message) {
    let future = send_message(socket, msg).map(|_| ()).map_err(|_| ());
    tokio::spawn(future);
}

pub fn cancel_connection(socket: TcpStream) {
    let future = send_message(socket, Message::ConnectCancel)
        .map_err(Error::from)
        .and_then(|framed_sock| {
            framed_sock
                .get_ref()
                .shutdown(Shutdown::Both)
                .map_err(Error::from)
        })
        .map(|_| ())
        .map_err(|err| {
            eprintln!(
                "Listener : Error when sending message `ConnectCancel` : `{:?}`.",
                err
            )
        });

    tokio::spawn(future);
}

#[derive(Debug)]
pub enum PingStatus {
    PingSent(Instant),
    PongReceived(Instant),
    NoPingYet,
}

impl PingStatus {
    pub fn new() -> Self {
        PingStatus::NoPingYet
    }

    pub fn is_ping_sent(&self) -> bool {
        if let PingStatus::PingSent(_) = self {
            true
        } else {
            false
        }
    }

    pub fn ping(&mut self) {
        *self = PingStatus::PingSent(Instant::now());
    }

    pub fn pong(&mut self) {
        *self = PingStatus::PongReceived(Instant::now());
    }
}

#[derive(Debug)]
pub enum PeerState {
    // TODO: ping uses this values?
    NotConnected,
    Connecting(ConnVote),
    // TODO: Connected(TcpStream)
    Connected(TcpStream),
}

impl Clone for PeerState {
    fn clone(&self) -> Self {
        use self::PeerState::*;
        match self {
            NotConnected => NotConnected,
            Connecting(connvote) => Connecting(*connvote),
            Connected(stream) => Connected(
                stream
                    .try_clone()
                    .expect("Could not clone stream when cloning PeerState"),
            ),
        }
    }
}

#[derive(Debug)]
pub struct Peer {
    // TODO: no `pub` ?
    // pid as option ?
    peer_pid: Pid,
    local_pid: Pid,
    peers: PeersMapArcMut,
    pub addr: SocketAddr,
    pub ping_status: PingStatus,
    // TODO: remove this todo if connection is done
    pub state: PeerState,
    // TODO: set to false when client socket error
    pub client_connecting: bool,
    // TODO: set to false when listener socket error...
    pub listener_connecting: bool,
}

impl Peer {
    // TODO: Use pid in constuctor
    pub fn new(local_pid: Pid, peer_pid: Pid, addr: SocketAddr, peers: PeersMapArcMut) -> Self {
        Peer {
            // TODO: do something with pid...
            peer_pid,
            local_pid,
            addr,
            peers,
            ping_status: PingStatus::new(),
            // TODO: remove this todo if connection is done
            state: PeerState::NotConnected,
            client_connecting: false,
            listener_connecting: false,
        }
    }

    pub fn is_connected(&self) -> bool {
        if let PeerState::Connected(_) = self.state {
            true
        } else {
            false
        }
    }

    pub fn is_ping_sent(&self) -> bool {
        self.ping_status.is_ping_sent()
    }

    pub fn ping(&mut self) {
        self.ping_status.ping()
    }

    pub fn pong(&mut self) {
        self.ping_status.pong()
    }

    pub fn to_connecting(&mut self) -> ConnVote {
        let local_vote = vote();
        self.state = PeerState::Connecting(local_vote);
        local_vote
    }

    pub fn listener_to_connecting(&mut self) -> ConnVote {
        self.listener_connecting = true;
        self.to_connecting()
    }

    pub fn client_to_connecting(&mut self) -> ConnVote {
        self.client_connecting = true;
        self.to_connecting()
    }

    // TODO: return Result ?
    /// **Important** : peer must be a reference to self.
    pub fn connected(&mut self, peer: PeerArcMut, socket: TcpStream) -> Result<(), Error> {
        // TODO: other checks ?
        self.listener_connecting = false;
        self.client_connecting = false;
        self.state = PeerState::Connected(socket);

        // TODO: make sure this is the last ref to socket?
        // TODO: start listening thread...
        println!("Connected to : `{}`", self.peer_pid);

        self.manage(peer);

        Ok(())
    }

    // TODO: too close names ? `{listener,client}_connection` ?
    /// **Important** : peer must be a reference to self.
    pub fn client_connection_acked(&mut self, peer: PeerArcMut, socket: TcpStream) {
        match self.connected(peer, socket) {
            Ok(()) => (),
            _ => unimplemented!(),
        }
    }

    pub fn listener_connection_ack(&mut self, peer: PeerArcMut, socket: TcpStream) {
        match self.connected(peer, socket) {
            Ok(()) => (),
            _ => unimplemented!(),
        }
        self.send_and_spawn(Message::ConnectAck);
    }

    // TODO: use that?
    pub fn client_connection_cancelled(&mut self) {
        self.client_connecting = false;

        if !self.listener_connecting {
            self.state = PeerState::NotConnected;
        }
    }

    pub fn client_connection_cancel(&mut self) {
        self.client_connecting = false;
        self.send_and_spawn(Message::ConnectCancel);

        if !self.listener_connecting {
            self.state = PeerState::NotConnected;
        }
    }

    pub fn listener_connection_cancel(&mut self) {
        self.listener_connecting = false;
        self.send_and_spawn(Message::ConnectCancel);

        if !self.client_connecting {
            self.state = PeerState::NotConnected;
        }
    }

    pub fn disconnect(&mut self) {
        self.ping_status = PingStatus::NoPingYet;
        self.state = PeerState::NotConnected;
        // TODO: disconnect message ?
    }

    // TODO: return Result ?
    pub fn set_pid(&mut self, pid: Pid) {
        unimplemented!();
        /*
        if let Some(present_pid) = self.peer_pid {
            eprintln!(
                "Attempting to write pid `{}`, but pid is already set `{}`.",
                pid, present_pid
            );
        } else {
            self.peer_pid = Some(pid);
        }
        */
    }

    /// Don't forget to spawn that...
    pub fn send(
        &mut self,
        msg: Message,
    ) -> impl Future<Item = Framed<TcpStream, MessageCodec>, Error = Error> {
        if let PeerState::Connected(socket) = &self.state {
            // TODO: unwrap?
            send_message(socket.try_clone().unwrap(), msg)
        } else {
            panic!(
                "Can't send `{:?}`, `peer.state` not in `Connected(socket)` !",
                msg
            );
        }
    }

    pub fn send_and_spawn(&mut self, msg: Message) {
        let future = self.send(msg).map(|_| ()).map_err(|_| ());
        tokio::spawn(future);
    }

    /// **Important** : peer must be a reference to self.
    pub fn manage(&mut self, peer: PeerArcMut) {
        if let PeerState::Connected(socket) = self.state.clone() {
            let framed_sock = Framed::new(socket, MessageCodec::new());
            let peer_addr = self.addr;

            let manage_future = framed_sock
                .map_err(Error::from)
                .for_each(move |msg| for_each_message(peer.clone(), &msg))
                .map_err(move |err| match err {
                    // TODO: println anyway ?
                    Error::ConnectionCancelled | Error::ConnectionEnded => (),
                    _ => eprintln!(
                        "Client : {} : error when receiving a message : {:?}.",
                        peer_addr, err
                    ),
                });

            tokio::spawn(manage_future);
        }
    }
}

fn for_each_message(peer: PeerArcMut, msg: &Message) -> Result<(), Error> {
    // TODO: lock for the whole function ?
    let mut peer = peer.lock().unwrap();

    match msg {
        Message::Ping => {
            // TODO: unwrap?
            let socket = {
                let socket = if let PeerState::Connected(socket) = &peer.state {
                    // TODO: unwrap?
                    socket.try_clone().unwrap()
                } else {
                    panic!("Inconsistent Peer object : a message was received, but `peer.state` is not `Connected(socket)`.");
                };

                socket
            };

            send_message_and_spawn(socket, Message::Ping);
        }
        Message::Pong => {
            // TODO: unwrap?
            peer.pong();
            // println!("{} : received Pong ! It is alive !!!", addr);
        }
        _ => println!(
            "{} : received a message (but won't do anything ;) !",
            peer.addr
        ),
    }

    Ok(())
}
