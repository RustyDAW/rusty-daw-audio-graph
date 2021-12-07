use basedrop::{Collector, Handle, Shared, SharedCell};
use rusty_daw_core::SampleRate;

use super::graph_state::GraphState;
use super::node::gain::{GainNodeUiHandle, StereoGainNode};
use super::resource_pool::GraphResourcePool;
use super::verifier::Verifier;
use super::{AudioGraphExecutor, AudioGraphNode, NodeRef, NodeState, PortType};
use super::{CompilerError, CompilerWarning};
use crate::resource_pool::{DebugNodeID, DebugNodeType};

pub struct GraphInterface<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    pub(crate) shared_graph_state:
        Shared<SharedCell<AudioGraphExecutor<GlobalData, MAX_BLOCKSIZE>>>,
    pub(crate) resource_pool: GraphResourcePool<GlobalData, MAX_BLOCKSIZE>,
    pub(crate) graph_state: GraphState,

    pub(crate) sample_rate: SampleRate,
    pub(crate) coll_handle: Handle,

    pub(crate) root_node_ref: NodeRef,
    pub(crate) _root_node_handle: GainNodeUiHandle,

    pub(crate) next_delay_comp_node_id: u64,
    pub(crate) next_sum_node_id: u64,

    pub(crate) verifier: Verifier<GlobalData, MAX_BLOCKSIZE>,
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    GraphInterface<GlobalData, MAX_BLOCKSIZE>
{
    pub fn new(
        sample_rate: SampleRate,
        coll_handle: Handle,
        global_data: GlobalData,
    ) -> (
        Self,
        Shared<SharedCell<AudioGraphExecutor<GlobalData, MAX_BLOCKSIZE>>>,
    ) {
        let collector = Collector::new();

        let (shared_graph_state, mut resource_pool) =
            AudioGraphExecutor::new(collector.handle(), sample_rate, global_data);
        let rt_shared_state = Shared::clone(&shared_graph_state);

        let mut graph_state = GraphState::new();

        let (root_node, _root_node_handle) = StereoGainNode::new(0.0, -90.0, 12.0, sample_rate);

        let root_replacing_ports = (
            <StereoGainNode<MAX_BLOCKSIZE> as AudioGraphNode<GlobalData, MAX_BLOCKSIZE>>::mono_replacing_ports(&root_node),
            <StereoGainNode<MAX_BLOCKSIZE> as AudioGraphNode<GlobalData, MAX_BLOCKSIZE>>::stereo_replacing_ports(&root_node),
        );

        let root_node_ref = graph_state.add_new_node(
            root_replacing_ports.0,
            <StereoGainNode<MAX_BLOCKSIZE> as AudioGraphNode<GlobalData, MAX_BLOCKSIZE>>::indep_mono_in_ports(&root_node),
            <StereoGainNode<MAX_BLOCKSIZE> as AudioGraphNode<GlobalData, MAX_BLOCKSIZE>>::indep_mono_out_ports(&root_node),
            root_replacing_ports.1,
            <StereoGainNode<MAX_BLOCKSIZE> as AudioGraphNode<GlobalData, MAX_BLOCKSIZE>>::indep_stereo_in_ports(&root_node),
            <StereoGainNode<MAX_BLOCKSIZE> as AudioGraphNode<GlobalData, MAX_BLOCKSIZE>>::indep_stereo_out_ports(&root_node),
        );

        resource_pool.add_user_node(
            root_node_ref,
            Box::new(root_node),
            DebugNodeID {
                node_type: DebugNodeType::Root,
                index: 0,
                name: None,
            },
            root_replacing_ports.0,
            root_replacing_ports.1,
        );

        (
            Self {
                shared_graph_state,
                resource_pool,
                graph_state,
                sample_rate,
                coll_handle,
                root_node_ref,
                _root_node_handle,
                next_delay_comp_node_id: 0,
                next_sum_node_id: 0,
                verifier: Verifier::new(),
            },
            rt_shared_state,
        )
    }

    // TODO: Some way to modify the delay compensation of nodes, which will cause the graph to recompile.

    // We are using a closure for all modifications to the graph instead of using individual methods to act on
    // the graph. This is so the graph only gets compiled once after the user is done, instead of being recompiled
    // after every method.
    // TODO: errors and reverting to previous working state
    pub fn modify_graph<F: FnOnce(GraphStateRef<'_, GlobalData, MAX_BLOCKSIZE>)>(
        &mut self,
        f: F,
    ) -> Result<Option<CompilerWarning>, CompilerError> {
        let graph_state_ref = GraphStateRef {
            resource_pool: &mut self.resource_pool,
            graph: &mut self.graph_state,
            root_node: self.root_node_ref,
            coll_handle: &self.coll_handle,
        };

        (f)(graph_state_ref);

        crate::compiler::compile_graph(self)
    }
}

pub struct GraphStateRef<'a, GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    resource_pool: &'a mut GraphResourcePool<GlobalData, MAX_BLOCKSIZE>,
    graph: &'a mut GraphState,
    root_node: NodeRef,
    coll_handle: &'a Handle,
}

impl<'a, GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    GraphStateRef<'a, GlobalData, MAX_BLOCKSIZE>
{
    pub fn add_new_node(
        &mut self,
        node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>>,
    ) -> NodeRef {
        let debug_name = node.debug_name();

        let mono_replacing_ports = node.mono_replacing_ports();
        let indep_mono_in_ports = node.indep_mono_in_ports();
        let indep_mono_out_ports = node.indep_mono_out_ports();
        let stereo_replacing_ports = node.stereo_replacing_ports();
        let indep_stereo_in_ports = node.indep_stereo_in_ports();
        let indep_stereo_out_ports = node.indep_stereo_out_ports();

        let delay = node.delay();

        let node_ref = self.graph.add_new_node(
            mono_replacing_ports,
            indep_mono_in_ports,
            indep_mono_out_ports,
            stereo_replacing_ports,
            indep_stereo_in_ports,
            indep_stereo_out_ports,
        );

        let index: usize = node_ref.into();
        let node_id = DebugNodeID {
            node_type: DebugNodeType::User,
            index: index as u64,
            name: Some(debug_name),
        };

        self.resource_pool.add_user_node(
            node_ref,
            node,
            node_id,
            mono_replacing_ports,
            stereo_replacing_ports,
        );

        log::debug!(
            "Added node to graph: node id: {:?} | # mono replacing: {} | # indep mono in: {} | # indep mono out: {} | # stereo replacing: {} | # indep stereo in {} | # indep stereo out: {}, delay: {}",
            node_id,
            mono_replacing_ports,
            indep_mono_in_ports,
            indep_mono_out_ports,
            stereo_replacing_ports,
            indep_stereo_in_ports,
            indep_stereo_out_ports,
            delay,
        );

        node_ref
    }

    /// Get information about the number of ports in a node.
    pub fn get_node_info(&self, node_ref: NodeRef) -> Result<&NodeState, audio_graph::Error> {
        self.graph.get_node_state(node_ref)
    }

    /// Get the root node.
    pub fn root_node(&self) -> NodeRef {
        self.root_node
    }

    /// Get the collector handle.
    pub fn coll_handle(&self) -> &Handle {
        self.coll_handle
    }

    /// Replace a node while attempting to keep previous connections.
    pub fn replace_node(
        &mut self,
        node_ref: NodeRef,
        new_node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>>,
    ) -> Result<(), audio_graph::Error> {
        // Don't allow replacing the root node.
        if node_ref == self.root_node {
            log::warn!("Attempted to replace the root node");

            return Err(audio_graph::Error::NodeDoesNotExist);
        }

        let node_index: usize = node_ref.into();
        let old_node = self
            .resource_pool
            .user_nodes
            .get(node_index)
            .ok_or_else(|| audio_graph::Error::NodeDoesNotExist)?;
        let old_node = &old_node
            .as_ref()
            .ok_or_else(|| audio_graph::Error::NodeDoesNotExist)?;

        let old_debug_id = old_node.node.debug_id();

        let new_debug_name = new_node.debug_name();

        let new_mono_replacing_ports = new_node.mono_replacing_ports();
        let new_indep_mono_in_ports = new_node.indep_mono_in_ports();
        let new_indep_mono_out_ports = new_node.indep_mono_out_ports();
        let new_stereo_replacing_ports = new_node.stereo_replacing_ports();
        let new_indep_stereo_in_ports = new_node.indep_stereo_in_ports();
        let new_indep_stereo_out_ports = new_node.indep_stereo_out_ports();

        let new_delay = new_node.delay();

        self.graph.set_num_ports(
            node_ref,
            new_mono_replacing_ports,
            new_indep_mono_in_ports,
            new_indep_mono_out_ports,
            new_stereo_replacing_ports,
            new_indep_stereo_in_ports,
            new_indep_stereo_out_ports,
        )?;

        let index: usize = node_ref.into();
        let new_node_id = DebugNodeID {
            node_type: DebugNodeType::User,
            index: index as u64,
            name: Some(new_debug_name),
        };

        self.resource_pool.remove_user_node(node_ref);
        self.resource_pool.add_user_node(
            node_ref,
            new_node,
            new_node_id,
            new_mono_replacing_ports,
            new_stereo_replacing_ports,
        );

        log::debug!(
            "Replaced node in graph: old node id: {:?} | new node id: {:?} | # mono replacing: {} | # indep mono in: {} | # indep mono out: {} | # stereo replacing: {} | # indep stereo in {} | # indep stereo out: {}, delay: {}",
            old_debug_id,
            new_node_id,
            new_mono_replacing_ports,
            new_indep_mono_in_ports,
            new_indep_mono_out_ports,
            new_stereo_replacing_ports,
            new_indep_stereo_in_ports,
            new_indep_stereo_out_ports,
            new_delay,
        );

        Ok(())
    }

    /// Remove a node from the graph.
    ///
    /// This will automatically remove all connections to this node as well.
    ///
    /// Please note that if this call was successful, then the given `node_ref` is now
    /// invalid and must be discarded.
    pub fn remove_node(&mut self, node_ref: NodeRef) -> Result<(), audio_graph::Error> {
        // Don't allow removing the root now.
        if node_ref == self.root_node {
            log::warn!("Attempted to remove the root node");

            return Err(audio_graph::Error::NodeDoesNotExist);
        }

        let node_index: usize = node_ref.into();
        let node = self
            .resource_pool
            .user_nodes
            .get(node_index)
            .ok_or_else(|| audio_graph::Error::NodeDoesNotExist)?;
        let node = &node
            .as_ref()
            .ok_or_else(|| audio_graph::Error::NodeDoesNotExist)?;

        let debug_id = node.node.debug_id();

        self.graph.remove_node(node_ref)?;
        self.resource_pool.remove_user_node(node_ref);

        log::debug!("Removed node from graph: node id: {:?}", debug_id);

        Ok(())
    }

    /// Add a connection between nodes.
    pub fn connect_ports(
        &mut self,
        port_type: PortType,
        source_node_ref: NodeRef,
        source_node_port_index: usize,
        dest_node_ref: NodeRef,
        dest_node_port_index: usize,
    ) -> Result<(), audio_graph::Error> {
        self.graph.connect_ports(
            port_type,
            source_node_ref,
            source_node_port_index,
            dest_node_ref,
            dest_node_port_index,
        )?;

        let src_node_index: usize = source_node_ref.into();
        let src_debug_id = self
            .resource_pool
            .user_nodes
            .get(src_node_index)
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .node
            .debug_id();

        let dest_node_index: usize = dest_node_ref.into();
        let dest_debug_id = self
            .resource_pool
            .user_nodes
            .get(dest_node_index)
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .node
            .debug_id();

        log::debug!(
            "Connect ports in graph: type: {:?} | source node id {:?}, source port index {} | -> | dest node id: {:?}, dest port index: {} |",
            port_type,
            src_debug_id,
            source_node_port_index,
            dest_debug_id,
            dest_node_port_index,
        );

        Ok(())
    }

    /// Remove a connection between nodes.
    pub fn disconnect_ports(
        &mut self,
        port_type: PortType,
        source_node_ref: NodeRef,
        source_node_port_index: usize,
        dest_node_ref: NodeRef,
        dest_node_port_index: usize,
    ) -> Result<(), audio_graph::Error> {
        self.graph.disconnect_ports(
            port_type,
            source_node_ref,
            source_node_port_index,
            dest_node_ref,
            dest_node_port_index,
        )?;

        let src_node_index: usize = source_node_ref.into();
        let src_debug_id = self
            .resource_pool
            .user_nodes
            .get(src_node_index)
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .node
            .debug_id();

        let dest_node_index: usize = dest_node_ref.into();
        let dest_debug_id = self
            .resource_pool
            .user_nodes
            .get(dest_node_index)
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .node
            .debug_id();

        log::debug!(
            "Disconnected ports in graph: type: {:?} | source node id {:?}, source port index {} | -> | dest node id: {:?}, dest port index: {} |",
            port_type,
            src_debug_id,
            source_node_port_index,
            dest_debug_id,
            dest_node_port_index,
        );

        Ok(())
    }
}
