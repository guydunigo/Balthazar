//! Classes for reperenting a job and its subtasks.
extern crate ethereum_types;
extern crate serde_derive;

use super::multiformats::{encode_multibase_multihash_string, DefaultHash};
use ethereum_types::Address;
use multiaddr::Multiaddr;
use multihash::Multihash;
use serde_derive::{Deserialize, Serialize};
use std::fmt;

// TODO: those are temporary aliases.
/// Identifies a unique job on the network.
pub type JobId = Multihash;
/// Identifies a unique task for a given job.
// TODO: should it also contain the job id ?
pub type TaskId = Multihash;

#[derive(Debug, Clone)]
pub struct UnknownValue<T>(T);

impl<T: fmt::Display> fmt::Display for UnknownValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unknown value: {}", self.0)
    }
}

impl<T: fmt::Debug + fmt::Display> std::error::Error for UnknownValue<T> {}

const BEST_METHOD_COST: u64 = 0;
const BEST_METHOD_PERFORMANCE: u64 = 1;
/// Method to choose which offer is the best to execute a task.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BestMethod {
    /// Choose the cheapest peer's offer.
    Cost,
    /// Choose the offer with the most performant worker.
    Performance,
}

impl Into<u64> for BestMethod {
    fn into(self) -> u64 {
        match self {
            BestMethod::Cost => BEST_METHOD_COST,
            BestMethod::Performance => BEST_METHOD_PERFORMANCE,
        }
    }
}

impl std::convert::TryFrom<u64> for BestMethod {
    type Error = UnknownValue<u64>;

    fn try_from(v: u64) -> Result<BestMethod, Self::Error> {
        match v {
            BEST_METHOD_COST => Ok(BestMethod::Cost),
            BEST_METHOD_PERFORMANCE => Ok(BestMethod::Performance),
            _ => Err(UnknownValue(v)),
        }
    }
}

impl fmt::Display for BestMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

const PROGRAM_KIND_WASM: u64 = 0;
/// Kind of program to execute.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ProgramKind {
    /// Webassembly program
    Wasm,
}

impl Into<u64> for ProgramKind {
    fn into(self) -> u64 {
        match self {
            ProgramKind::Wasm => PROGRAM_KIND_WASM,
        }
    }
}

impl std::convert::TryFrom<u64> for ProgramKind {
    type Error = UnknownValue<u64>;

    fn try_from(v: u64) -> Result<ProgramKind, Self::Error> {
        match v {
            PROGRAM_KIND_WASM => Ok(ProgramKind::Wasm),
            _ => Err(UnknownValue(v)),
        }
    }
}

impl fmt::Display for ProgramKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Description of a Job.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub program_kind: ProgramKind,
    pub addresses: Vec<Multiaddr>,
    pub program_hash: Multihash,
    pub arguments: Vec<Vec<u8>>,

    pub timeout: u64,
    pub max_failures: u64,
    pub best_method: BestMethod,
    pub max_worker_price: u64,
    pub min_cpu_count: u64,
    pub min_memory: u64,
    pub max_network_usage: u64,
    pub max_network_price: u64,
    pub min_network_speed: u64,

    pub redundancy: u64,
    pub is_program_pure: bool,

    pub sender: Address,
    /// `None` if the job hasn't been sent yet or isn't known.
    pub nonce: Option<u128>,
}

impl fmt::Display for Job {
    #[allow(irrefutable_let_patterns)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "---------")?;
        write!(f, "Job id: ")?;
        if self.nonce.is_some() {
            writeln!(
                f,
                "{}",
                encode_multibase_multihash_string(&self.job_id().expect("Already checked option."))
            )?;
        } else {
            writeln!(f, "Unknown")?;
        }
        writeln!(f, "Program kind: {}", self.program_kind)?;
        writeln!(
            f,
            "Program hash: {}",
            encode_multibase_multihash_string(&self.program_hash)
        )?;
        writeln!(f, "Addresses: {:?}", self.addresses)?;
        writeln!(f, "Arguments: [")?;
        for (i, a) in self.arguments.iter().enumerate() {
            write!(f, "  ")?;
            if let Some(job_id) = self.job_id() {
                write!(
                    f,
                    "{}",
                    encode_multibase_multihash_string(&task_id(&job_id, i as u128, &a[..]))
                )?;
            } else {
                write!(f, "{}", i)?;
            }
            writeln!(f, ": {}", String::from_utf8_lossy(&a[..]))?;
        }
        writeln!(f, "]")?;
        writeln!(f)?;
        writeln!(f, "Timeout: {}s", self.timeout)?;
        writeln!(f, "Max failures: {}", self.max_failures)?;
        writeln!(f)?;
        writeln!(f, "Best method: {}", self.best_method)?;
        writeln!(f, "Max worker price: {} money/s", self.max_worker_price)?;
        writeln!(f, "Min CPU count: {}", self.min_cpu_count)?;
        writeln!(f, "Min memory: {} kilobytes", self.min_memory)?;
        writeln!(f, "Max network usage: {} kilobits", self.max_network_usage)?;
        writeln!(
            f,
            "Max network price: {} money/kilobits",
            self.max_network_price
        )?;
        writeln!(
            f,
            "Min network speed: {} kilobits/s",
            self.min_network_speed
        )?;
        writeln!(f)?;
        writeln!(f, "Redundancy: {}", self.redundancy)?;
        writeln!(
            f,
            "Is program pure? {}",
            if self.is_program_pure { "Yes" } else { "No" }
        )?;
        writeln!(f)?;
        writeln!(f, "Sender: {}", self.sender)?;
        write!(f, "Nonce: ")?;
        if let Some(nonce) = &self.nonce {
            writeln!(f, "{}", nonce)?;
        } else {
            writeln!(f, "Unknown")?;
        }
        writeln!(f)?;
        writeln!(f, "Max price: {} money", self.calc_max_price())?;
        writeln!(f, "---------")
    }
}

impl Job {
    /// Calculate job id of current job if nonce is set.
    pub fn job_id(&self) -> Option<JobId> {
        if let Some(nonce) = self.nonce {
            Some(job_id(&self.sender, nonce))
        } else {
            None
        }
    }

    pub fn calc_max_price(&self) -> u64 {
        self.redundancy
            * self.addresses.len() as u64
            * (self.timeout * self.max_worker_price
                + self.max_network_usage * self.max_network_price)
    }
}

/// Calculate JobId.
pub fn job_id(address: &Address, nonce: u128) -> JobId {
    let mut buffer = Vec::with_capacity(address.0.len() + 16);
    buffer.extend_from_slice(&address[..]);
    buffer.extend_from_slice(&nonce.to_le_bytes()[..]);
    DefaultHash::digest(&buffer[..])
}

/// Calculate TaskId.
pub fn task_id(job_id: &Multihash, i: u128, argument: &[u8]) -> TaskId {
    let mut buffer = Vec::with_capacity(job_id.digest().len() + argument.len());
    buffer.extend_from_slice(job_id.digest());
    buffer.extend_from_slice(&i.to_le_bytes()[..]);
    buffer.extend_from_slice(&argument[..]);
    DefaultHash::digest(&buffer[..])
}

/*
pub struct Task {
    pub task_id: TaskId,
    pub arguments: Vec<u8>,
}
*/
