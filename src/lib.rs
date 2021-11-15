mod error;
mod executor;
mod graph_state;
mod interface;
mod proc_buffers;
mod schedule;
mod task;

pub mod node;

pub(crate) mod compiler;
pub(crate) mod resource_pool;
pub(crate) mod shared;
pub(crate) mod verifier;

pub use audio_graph::NodeRef;

pub use error::*;
pub use executor::AudioGraphExecutor;
pub use graph_state::{NodeState, PortIdent, PortPlacement, PortType};
pub use interface::{GraphInterface, GraphStateRef};
pub use node::AudioGraphNode;
pub use proc_buffers::{MonoProcBuffer, ProcBuffers, StereoProcBuffer};
pub use resource_pool::{DebugBufferID, DebugBufferType, DebugNodeID, DebugNodeType};
pub use schedule::{ProcInfo, Schedule};

pub const SMALLVEC_ALLOC_PORT_BUFFERS: usize = 4;
pub const SMALLVEC_ALLOC_MPR_BUFFERS: usize = 4;

// TODO: Write tests
