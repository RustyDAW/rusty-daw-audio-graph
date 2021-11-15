use crate::resource_pool::{DebugBufferID, DebugNodeID};

/// A non-fatal warning from the compiler.
///
/// This is returned whenever the graph is technically valid, but may
/// not have the behavior the user intended.
#[derive(Debug, Clone)]
pub enum CompilerWarning {
    /// The master output buffer was not found in the schedule because no nodes
    /// in the graph are connected to the root node.
    ///
    /// This will result in an output of silence.
    NothingConnectedToRoot,
}

impl std::error::Error for CompilerWarning {}

impl std::fmt::Display for CompilerWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompilerWarning::NothingConnectedToRoot => {
                write!(f, "compiler error: master output buffer was not found because no nodes are connected to root")
            }
        }
    }
}

/// An internal error that happened in the compiler.
///
/// In theory these errors should never happen, but because stability is
/// a high priority, our code must be resiliant to any possible bugs.
///
/// If one of these errors is returned, then it should be handled by
/// reverting the graph back to its previous working state (because
/// we really want to avoid crashing in the middle of a user's session).
#[derive(Debug, Clone)]
pub enum CompilerError {
    /// The user node with the given index does not exist.
    UserNodeDoesNotExist(usize),
    /// The same node has appeared twice in the schedule.
    NodeAppearsTwice(DebugNodeID),
    /// The same buffer has appeared twice in the same task.
    BufferAppearsTwice(DebugBufferID),
}

impl std::error::Error for CompilerError {}

impl std::fmt::Display for CompilerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompilerError::UserNodeDoesNotExist(index) => {
                write!(
                    f,
                    "Compiler error: the user node with the index {} does not exist",
                    index
                )
            }
            CompilerError::NodeAppearsTwice(node_id) => {
                write!(
                    f,
                    "Compiler error: the same node has appeared twice in the schedule: {:?}",
                    node_id
                )
            }
            CompilerError::BufferAppearsTwice(buf_id) => {
                write!(
                    f,
                    "Compiler error: the same buffer has appeared twice in the same task: {:?}",
                    buf_id
                )
            }
        }
    }
}
