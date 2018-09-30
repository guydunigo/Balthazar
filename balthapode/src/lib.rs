extern crate balthajob as job;
extern crate balthmessage as message;
extern crate parity_wasm;
extern crate wasmi;

mod orchestrator;

//TODO: +everywhere stream or socket or ...

use std::convert::From;
use std::fmt::Display;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::Mutex;

use job::Job;
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
        let jobs: Vec<Arc<Mutex<Job<bool>>>> = Vec::new();
        let jobs = Arc::new(Mutex::new(jobs));

        orchestrator::start_orchestrator(jobs.clone());

        let mut f = File::open("main.wasm")?;
        let mut code: Vec<u8> = Vec::new();
        f.read_to_end(&mut code)?;

        Message::Job(0, code).send(&mut socket)?;

        //let mut socket = socket.try_clone()?;
        Message::Idle(1).send(&mut socket)?;
        reader.for_each_until_error(|msg| match msg {
            Message::Job(job_id, bytecode) => {
                // TODO: multiple jobs having same id ?
                // The use of `is_none` is due to `jobs` being borrowed...
                let job_opt = match jobs
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|j| j.lock().unwrap().id == job_id)
                {
                    Some(job) => Some(job.clone()),
                    None => None,
                };

                match job_opt {
                    Some(job) => job.lock().unwrap().set_bytecode(bytecode),
                    None => jobs
                        .lock()
                        .unwrap()
                        .push(Arc::new(Mutex::new(Job::new(job_id, bytecode)))),
                }

                Ok(())
            }
            Message::Task(job_id, task_id, args) => {
                //TODO: use balthajob to represent jobs and tasks and execute them there.
                //TODO: do not fail on job error
                let job_opt = match jobs
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|j| j.lock().unwrap().id == job_id)
                {
                    Some(job) => Some(job.clone()),
                    None => None,
                };

                let job = match job_opt {
                    Some(job) => job,
                    None => {
                        let job = Arc::new(Mutex::new(Job::new(job_id, Vec::new())));
                        jobs.lock().unwrap().push(job.clone());
                        Message::RequestJob(job_id).send(&mut socket)?;
                        job
                    }
                };

                job.lock().unwrap().push_task(task_id, args);
                println!("Task #{} for Job #{} saved", task_id, job_id);

                Ok(())
            }
            _ => {
                Message::Disconnect.send(&mut socket)
                //Ok(())
            }
        })
    };

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
