use std::io;
use std::io::prelude::*;
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

// TODO: replace TcpStream by Read + Write

use message::{de, ser, Message, MessageReader};

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    OrchestratorTxError(mpsc::SendError<Message>),
    SerError(ser::Error),
    DeError(de::Error),
    AlreadyManagedError,
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

impl From<mpsc::SendError<Message>> for Error {
    fn from(err: mpsc::SendError<Message>) -> Error {
        Error::OrchestratorTxError(err)
    }
}

impl From<ser::Error> for Error {
    fn from(err: ser::Error) -> Error {
        Error::SerError(err)
    }
}

impl From<de::Error> for Error {
    fn from(err: de::Error) -> Error {
        Error::DeError(err)
    }
}

pub struct Manager {
    id: usize,
    stream: Option<TcpStream>,
    orch_tx: Option<mpsc::Sender<Message>>,
    handle: Option<thread::JoinHandle<Result<(), Error>>>,
    _job: Option<Arc<()>>,
}

impl Manager {
    pub fn new(id: usize, stream: TcpStream, orch_tx: mpsc::Sender<Message>) -> Manager {
        Manager {
            id,
            stream: Some(stream),
            orch_tx: Some(orch_tx),
            handle: None,
            _job: None,
        }
    }

    pub fn manage(&mut self) -> Result<(), Error> {
        if let (None, Some(stream), Some(orch_tx)) =
            (&self.handle, self.stream.take(), self.orch_tx.take())
        {
            let id = self.id;
            self.handle = Some(thread::spawn(move || manage(id, stream, orch_tx)));
        } else {
            return Err(Error::AlreadyManagedError);
        }

        Ok(())
    }
}

pub fn manage(
    id: usize,
    mut stream: TcpStream,
    orch_tx: mpsc::Sender<Message>,
) -> Result<(), Error> {
    let peer_addr = stream.peer_addr()?;
    println!("New Pode {} at address : `{}`", id, peer_addr);

    let id_msg = Message::Connected(id);
    let msg_str = ser::to_string(&id_msg)?;
    stream.write_all(msg_str.as_bytes())?;

    let reader = MessageReader::new(id, stream.try_clone()?);
    reader
        .map(|msg_res| -> Result<(), Error> {
            match msg_res {
                Ok(msg) => {
                    println!("Received : `{:?}`", msg);
                }
                Err(err) => return Err(Error::from(err)),
            }

            let msg = Message::Hello("salut".to_string());
            let msg_str = ser::to_string(&msg)?;

            stream.write_all(msg_str.as_bytes())?;
            Ok(())
        })
        .skip_while(|result| result.is_ok())
        .next()
        .unwrap()?;

    // println!("Manager {} : Disconnected, notifying orchestrator...", id);
    // TODO: Report errors ?
    orch_tx.send(Message::Disconnected(id))?;

    Ok(())
}

impl Drop for Manager {
    fn drop(&mut self) {
        // println!("Manager {} : Dropping...", self.id);

        if let Some(handle) = self.handle.take() {
            // println!("Manager {} : Joining the thread...", self.id);
            handle.join().unwrap().unwrap();
        } else {
            // println!("Manager {} : Closing the stream...", self.id);
            self.stream
                .take()
                .unwrap()
                .shutdown(Shutdown::Both)
                .unwrap();
        }

        // println!("Manager {} : Deleted", self.id);
    }
}
