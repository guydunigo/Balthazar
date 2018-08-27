extern crate balthmessage as message;
//TODO: +everywhere stream or socket or ...

use std::convert::From;
use std::fmt::Display;
use std::io;
use std::net::{TcpStream, ToSocketAddrs};

use message::{Message, MessageReader};

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    FailedHandshake,
    IoError(io::Error),
    MessageError(message::Error),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

impl From<message::Error> for Error {
    fn from(err: message::Error) -> Error {
        Error::MessageError(err)
    }
}

// ------------------------------------------------------------------

pub fn swim<A: ToSocketAddrs + Display>(addr: A) -> Result<(), Error> {
    let mut socket = TcpStream::connect(&addr)?;
    println!("Connected to : `{}`", addr);

    //TODO: as option
    let id = {
        let mut init_reader = MessageReader::new(0, socket.try_clone()?);
        match init_reader.next() {
            Some(Ok(Message::Connected(id))) => Ok(id),
            _ => Err(Error::FailedHandshake),
        }
    }?;
    println!("Handshake successful, received id : {}.", id);

    let mut reader = MessageReader::new(id, socket.try_clone()?);
    let result = {
        //let mut socket = socket.try_clone()?;
        Message::Idle(1).send(&mut socket)?;
        match reader.next() {
            Some(Ok(Message::Job(_))) => {
                println!("Pode received a job !");
                Ok(())
            }
            Some(Ok(Message::Disconnected(_))) => {
                println!("Disconnected");
                Ok(())
            }
            Some(Ok(msg)) => {
                println!("Recieved {:?}, quitting...", msg);
                Message::Disconnect.send(&mut socket)
            }
            _ => {
                println!("Recieved something, quitting...");
                Message::Disconnect.send(&mut socket)
            }
        }
    };

    // Message::Connected(id).send(&mut socket)?;

    match result {
        Err(err) => Err(Error::from(err)),
        Ok(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}