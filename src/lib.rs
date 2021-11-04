mod graph_state;
mod interface;
mod schedule;
mod task;

pub mod node;

pub(crate) mod compiler;
pub(crate) mod resource_pool;

pub use audio_graph::NodeRef;

pub use compiler::CompiledGraph;
pub use graph_state::{NodeState, PortIdent, PortPlacement, PortType};
pub use interface::{GraphInterface, GraphStateRef};
pub use node::AudioGraphNode;
pub use resource_pool::{DebugBufferID, DebugNodeID};
pub use schedule::{ProcInfo, Schedule};
pub use task::{
    AudioGraphTask, MonoProcBuffers, MonoProcBuffersMut, ProcBuffers, StereoProcBuffers,
    StereoProcBuffersMut,
};

pub const SMALLVEC_ALLOC_BUFFERS: usize = 4;
pub const SMALLVEC_ALLOC_MPR_BUFFERS: usize = 4;

// TODO: Write tests
