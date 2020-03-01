//! Grouping module for all balthazar sub-modules.
use futures::{
    channel::mpsc::{channel, Sender},
    future, join, FutureExt, SinkExt, StreamExt,
};
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{runtime::Runtime, sync::RwLock};

use chain::{Chain, JobsEvent};
use misc::{
    job::{JobId, TaskId},
    WorkerSpecs,
};
use proto::{
    worker::{TaskErrorKind, TaskExecute},
    NodeType, TaskStatus,
};
use run::{Runner, WasmRunner};
use store::{Storage, StoragesWrapper};

use super::{BalthazarConfig, Error};

const CHANNEL_SIZE: usize = 1024;

pub fn run(config: BalthazarConfig) -> Result<(), Error> {
    Runtime::new().unwrap().block_on(Balthazar::run(config))
}

/*
// TODO: cleaner and in self module
async fn get_keypair(keyfile_path: &Path) -> Result<Keypair, Error> {
    let mut bytes = fs::read(keyfile_path)
        .await
        .map_err(Error::KeyPairReadFileError)?;
    Keypair::rsa_from_pkcs8(&mut bytes).map_err(Error::KeyPairDecodingError)
}
*/

#[derive(Debug)]
enum Event {
    ChainJobs(chain::JobsEvent),
    Swarm(net::EventOut),
    Error(Error),
    Log { kind: LogKind, msg: String },
}

impl fmt::Display for Event {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::Log { kind, msg } => write!(fmt, "{} --- {}", kind, msg),
            _ => write!(fmt, "{:?}", self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum LogKind {
    Swarm,
    Worker,
    Manager,
    Blockchain,
    Error,
}

impl fmt::Display for LogKind {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let letter = match self {
            LogKind::Swarm => "S",
            LogKind::Worker => "W",
            LogKind::Manager => "M",
            LogKind::Blockchain => "B",
            LogKind::Error => "E",
        };
        write!(fmt, "{}", letter)
    }
}

#[derive(Clone)]
struct Balthazar {
    config: Arc<BalthazarConfig>,
    tx: Sender<Event>,
    swarm_in: Sender<net::EventIn>,
    pending_tasks: Arc<RwLock<VecDeque<(JobId, TaskId)>>>,
}
/*
{
    keypair: balthernet::identity::Keypair,
    swarm_in: Sender<net::EventIn>,
    store: StoragesWrapper,
}
*/

async fn spawn_log(tx: Sender<Event>, kind: LogKind, msg: String) {
    spawn_event(tx, Event::Log { kind, msg }).await
}

async fn spawn_event(mut tx: Sender<Event>, event: Event) {
    if let Err(e) = tx.send(event).await {
        panic!("{:?}", Error::EventChannelError(e));
    }
}

impl Balthazar {
    fn new(config: BalthazarConfig, tx: Sender<Event>, swarm_in: Sender<net::EventIn>) -> Self {
        Balthazar {
            config: Arc::new(config),
            tx,
            swarm_in,
            pending_tasks: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    async fn pending_tasks<'a>(
        &'a self,
    ) -> impl 'a + std::ops::Deref<Target = VecDeque<(JobId, TaskId)>> {
        self.pending_tasks.read().await
    }

    async fn pending_tasks_mut<'a>(
        &'a self,
    ) -> impl 'a + std::ops::DerefMut<Target = VecDeque<(JobId, TaskId)>> {
        self.pending_tasks.write().await
    }

    fn chain(&self) -> Chain {
        Chain::new(self.config.chain())
    }

    async fn send_msg_to_behaviour(&mut self, event: net::EventIn) {
        if let Err(e) = self.swarm_in.send(event).await {
            panic!("{:?}", Error::SwarmChannelError(e));
        }
    }

    pub async fn run(config: BalthazarConfig) -> Result<(), Error> {
        println!("Starting as {:?}...", config.node_type());

        let (tx, rx) = channel(CHANNEL_SIZE);

        let specs = WorkerSpecs::default();
        let keypair = balthernet::identity::Keypair::generate_secp256k1();
        let (swarm_in, swarm_out) = net::get_swarm(keypair.clone(), config.net(), Some(&specs));

        let balth = Balthazar::new(config, tx, swarm_in);

        let chain = balth.chain();
        let chain_fut = chain.jobs_subscribe().await?.for_each(|e| {
            async {
                let event = match e {
                    Ok(evt) => Event::ChainJobs(evt),
                    Err(e) => Event::Error(e.into()),
                };
                spawn_event(balth.tx.clone(), event).await;
            }
            .boxed()
        });

        let swarm_fut = swarm_out.for_each(|e| spawn_event(balth.tx.clone(), Event::Swarm(e)));

        // TODO: concurrent ?
        let channel_fut = rx.for_each(|e| balth.clone().handle_event(e));

        join!(chain_fut, swarm_fut, channel_fut);

        Ok(())
    }

    async fn spawn_log(&self, kind: LogKind, msg: String) {
        spawn_log(self.tx.clone(), kind, msg).await;
    }

    /// Handle all inner events.
    async fn handle_event(self, event: Event) {
        match event {
            Event::Swarm(e) => self.handle_swarm_event(e).await,
            Event::ChainJobs(e) => self.handle_chain_event(e).await,
            Event::Log { .. } => future::ready(eprintln!("{}", event)).await,
            _ => unimplemented!(),
        }
    }

    /// Handle events coming out of Swarm.
    async fn handle_chain_event(self, event: chain::JobsEvent) {
        self.spawn_log(LogKind::Blockchain, format!("{}", event))
            .await;
        match event {
            chain::JobsEvent::JobLocked { job_id } => {
                let mut p = self.pending_tasks_mut().await;
                self.chain()
                    .jobs_get_all_arguments(job_id)
                    .await
                    .iter()
                    .enumerate()
                    .for_each(|(task_id, _)| p.push_front((job_id, task_id as u64)));
            }
            _ => (),
        }
    }

    /// Handle events coming out of Swarm.
    async fn handle_swarm_event(mut self, event: net::EventOut) {
        match (self.config.node_type(), event) {
            (NodeType::Manager, net::EventOut::WorkerNew(peer_id)) => {
                if let Some((wasm, args)) = self.config.wasm() {
                    let mut tasks = HashMap::new();
                    tasks.insert(
                        wasm.clone(),
                        TaskExecute {
                            job_id: wasm.clone(),
                            task_id: wasm.clone(),
                            job_addr: vec![wasm.clone()],
                            arguments: args.clone(),
                            timeout: 100,
                        },
                    );
                    spawn_log(
                        self.tx.clone(),
                        LogKind::Manager,
                        format!(
                            "Sending task `{}` with parameters `{}` to worker `{}`",
                            String::from_utf8_lossy(wasm),
                            String::from_utf8_lossy(args),
                            peer_id
                        ),
                    )
                    .await;
                    self.send_msg_to_behaviour(net::EventIn::TasksExecute(peer_id, tasks))
                        .await
                }
            }
            (
                NodeType::Manager,
                net::EventOut::TaskStatus {
                    peer_id,
                    task_id,
                    status,
                },
            ) => {
                spawn_log(
                    self.tx.clone(),
                    LogKind::Manager,
                    format!(
                        "Task status from peer `{}` for task `{}`: `{:?}`",
                        peer_id,
                        String::from_utf8_lossy(&task_id[..]),
                        status
                    ),
                )
                .await;
            }
            (NodeType::Worker, net::EventOut::TasksExecute(tasks)) => {
                for task in tasks.values() {
                    self.send_msg_to_behaviour(net::EventIn::TaskStatus(
                        task.task_id.clone(),
                        TaskStatus::Pending,
                    ))
                    .await;
                    let storage = StoragesWrapper::default();
                    let string_job_addr = String::from_utf8_lossy(&task.job_addr[0][..]);
                    let string_arguments = String::from_utf8_lossy(&task.arguments[..]);

                    self.spawn_log(
                        LogKind::Worker,
                        format!("will get program `{}`...", string_job_addr),
                    )
                    .await;
                    match storage.get(&task.job_addr[0][..]).await {
                        Ok(wasm) => {
                            self.spawn_log(
                                LogKind::Worker,
                                format!("received program `{}`.", string_job_addr),
                            )
                            .await;
                            self.spawn_log(
                                LogKind::Worker,
                                format!(
                                    "spawning wasm executor for `{}` with argument `{}`...",
                                    string_job_addr, string_arguments,
                                ),
                            )
                            .await;

                            self.send_msg_to_behaviour(net::EventIn::TaskStatus(
                                task.task_id.clone(),
                                TaskStatus::Started(
                                    SystemTime::now()
                                        .duration_since(UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs() as i64,
                                ),
                            ))
                            .await;

                            match WasmRunner::run_async(&wasm[..], &task.arguments[..]).await {
                                Ok(result) => {
                                    self.spawn_log(
                                        LogKind::Worker,
                                        format!(
                                            "task result for `{}` with `{}`: `{:?}`",
                                            string_job_addr,
                                            string_arguments,
                                            String::from_utf8_lossy(&result[..])
                                        ),
                                    )
                                    .await;
                                    self.send_msg_to_behaviour(net::EventIn::TaskStatus(
                                        task.task_id.clone(),
                                        TaskStatus::Completed(result),
                                    ))
                                    .await;
                                }
                                Err(error) => {
                                    self.send_msg_to_behaviour(net::EventIn::TaskStatus(
                                        task.task_id.clone(),
                                        TaskStatus::Error(TaskErrorKind::Running),
                                    ))
                                    .await;
                                    self.spawn_log(
                                        LogKind::Worker,
                                        format!(
                                            "task error for `{}` with `{}`: `{:?}`",
                                            string_job_addr, string_arguments, error
                                        ),
                                    )
                                    .await;
                                }
                            }
                        }
                        Err(error) => {
                            self.send_msg_to_behaviour(net::EventIn::TaskStatus(
                                task.task_id.clone(),
                                TaskStatus::Error(TaskErrorKind::Download),
                            ))
                            .await;
                            self.spawn_log(
                                LogKind::Worker,
                                format!(
                                    "error while fetching `{}`: `{:?}`",
                                    string_job_addr, error
                                ),
                            )
                            .await;
                        }
                    }
                }
            }
            (_, event) => {
                spawn_log(
                    self.tx.clone(),
                    LogKind::Swarm,
                    format!("event: {:?}", event),
                )
                .await;
            }
        }
    }
}
