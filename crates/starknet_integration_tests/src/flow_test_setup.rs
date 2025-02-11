use std::net::SocketAddr;

use blockifier::context::ChainInfo;
use mempool_test_utils::starknet_api_test_utils::{
    AccountTransactionGenerator,
    MultiAccountTransactionGenerator,
};
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::test_utils::{
    create_connected_network_configs,
    network_config_into_broadcast_channels,
};
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::{HeightAndRound, ProposalPart, StreamMessage};
use papyrus_storage::StorageConfig;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_http_server::config::HttpServerConfig;
use starknet_http_server::test_utils::HttpTestClient;
use starknet_infra_utils::test_utils::AvailablePorts;
use starknet_mempool_p2p::config::MempoolP2pConfig;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_monitoring_endpoint::test_utils::MonitoringClient;
use starknet_sequencer_node::clients::SequencerNodeClients;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use starknet_sequencer_node::servers::run_component_servers;
use starknet_sequencer_node::utils::create_node_modules;
use starknet_state_sync::config::StateSyncConfig;
use tempfile::TempDir;
use tracing::{debug, instrument};

use crate::integration_test_setup::NodeExecutionId;
use crate::state_reader::StorageTestSetup;
use crate::utils::{
    create_chain_info,
    create_consensus_manager_configs_from_network_configs,
    create_mempool_p2p_configs,
    create_node_config,
    create_state_sync_configs,
    spawn_local_success_recorder,
};

const SEQUENCER_0: usize = 0;
const SEQUENCER_1: usize = 1;
const SEQUENCER_INDICES: [usize; 2] = [SEQUENCER_0, SEQUENCER_1];

pub struct FlowTestSetup {
    pub sequencer_0: FlowSequencerSetup,
    pub sequencer_1: FlowSequencerSetup,

    // Channels for consensus proposals, used for asserting the right transactions are proposed.
    pub consensus_proposals_channels:
        BroadcastTopicChannels<StreamMessage<ProposalPart, HeightAndRound>>,
}

impl FlowTestSetup {
    #[instrument(skip(tx_generator), level = "debug")]
    pub async fn new_from_tx_generator(
        tx_generator: &MultiAccountTransactionGenerator,
        test_unique_index: u16,
    ) -> Self {
        let chain_info = create_chain_info();
        let mut available_ports = AvailablePorts::new(test_unique_index, 0);

        let accounts = tx_generator.accounts();
        let (consensus_manager_configs, consensus_proposals_channels) =
            create_consensus_manager_configs_and_channels(
                available_ports.get_next_ports(SEQUENCER_INDICES.len() + 1),
            );
        let [sequencer_0_consensus_manager_config, sequencer_1_consensus_manager_config]: [ConsensusManagerConfig;
            2] = consensus_manager_configs.try_into().unwrap();

        let ports = available_ports.get_next_ports(SEQUENCER_INDICES.len());
        let mempool_p2p_configs = create_mempool_p2p_configs(chain_info.chain_id.clone(), ports);
        let [sequencer_0_mempool_p2p_config, sequencer_1_mempool_p2p_config]: [MempoolP2pConfig;
            2] = mempool_p2p_configs.try_into().unwrap();

        let [sequencer_0_state_sync_config, sequencer_1_state_sync_config]: [StateSyncConfig; 2] =
            create_state_sync_configs(StorageConfig::default(), available_ports.get_next_ports(2))
                .try_into()
                .unwrap();

        // Create nodes one after the other in order to make sure the ports are not overlapping.
        let sequencer_0 = FlowSequencerSetup::new(
            accounts.to_vec(),
            SEQUENCER_0,
            chain_info.clone(),
            sequencer_0_consensus_manager_config,
            sequencer_0_mempool_p2p_config,
            AvailablePorts::new(test_unique_index, 1),
            sequencer_0_state_sync_config,
        )
        .await;

        let sequencer_1 = FlowSequencerSetup::new(
            accounts.to_vec(),
            SEQUENCER_1,
            chain_info,
            sequencer_1_consensus_manager_config,
            sequencer_1_mempool_p2p_config,
            AvailablePorts::new(test_unique_index, 2),
            sequencer_1_state_sync_config,
        )
        .await;

        Self { sequencer_0, sequencer_1, consensus_proposals_channels }
    }

    pub async fn assert_add_tx_error(&self, tx: RpcTransaction) -> GatewaySpecError {
        self.sequencer_0.add_tx_http_client.assert_add_tx_error(tx).await
    }
}

pub struct FlowSequencerSetup {
    /// Used to differentiate between different sequencer nodes.
    pub node_index: usize,

    // Client for adding transactions to the sequencer node.
    pub add_tx_http_client: HttpTestClient,

    // Handlers for the storage files, maintained so the files are not deleted.
    pub batcher_storage_file_handle: Option<TempDir>,
    pub state_sync_storage_file_handle: Option<TempDir>,
    pub class_manager_storage_file_handles: Option<starknet_class_manager::test_utils::FileHandles>,

    // Node configuration.
    pub node_config: SequencerNodeConfig,

    // Monitoring client.
    pub monitoring_client: MonitoringClient,

    // Retain clients to avoid closing communication channels, which crashes the server and
    // subsequently the test. This occurs for components who are wrapped by servers, but no other
    // component has their client, usually due to these clients being added in a later date.
    _clients: SequencerNodeClients,
}

impl FlowSequencerSetup {
    #[instrument(skip(accounts, chain_info, consensus_manager_config), level = "debug")]
    pub async fn new(
        accounts: Vec<AccountTransactionGenerator>,
        node_index: usize,
        chain_info: ChainInfo,
        mut consensus_manager_config: ConsensusManagerConfig,
        mempool_p2p_config: MempoolP2pConfig,
        mut available_ports: AvailablePorts,
        state_sync_config: StateSyncConfig,
    ) -> Self {
        let path = None;
        let StorageTestSetup {
            batcher_storage_config,
            batcher_storage_handle,
            state_sync_storage_config,
            state_sync_storage_handle,
            class_manager_storage_config,
            class_manager_storage_handles,
        } = StorageTestSetup::new(accounts, &chain_info, path);

        let (recorder_url, _join_handle) =
            spawn_local_success_recorder(available_ports.get_next_port());
        consensus_manager_config.cende_config.recorder_url = recorder_url;

        let component_config = ComponentConfig::default();

        // Explicitly avoid collecting metrics in the monitoring endpoint; metrics are collected
        // using a global recorder, which fails when being set multiple times in the same
        // process, as in this test.
        let monitoring_endpoint_config = MonitoringEndpointConfig {
            port: available_ports.get_next_port(),
            collect_metrics: false,
            ..Default::default()
        };

        // Derive the configuration for the sequencer node.
        let (node_config, _required_params) = create_node_config(
            &mut available_ports,
            NodeExecutionId::new(node_index, 0),
            chain_info,
            batcher_storage_config,
            state_sync_storage_config,
            class_manager_storage_config,
            state_sync_config,
            consensus_manager_config,
            mempool_p2p_config,
            monitoring_endpoint_config,
            component_config,
        );

        debug!("Sequencer config: {:#?}", node_config);
        let (_clients, servers) = create_node_modules(&node_config).await;

        let MonitoringEndpointConfig { ip, port, .. } = node_config.monitoring_endpoint_config;
        let monitoring_client = MonitoringClient::new(SocketAddr::from((ip, port)));

        let HttpServerConfig { ip, port } = node_config.http_server_config;
        let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        // Run the sequencer node.
        tokio::spawn(run_component_servers(servers));

        Self {
            node_index,
            add_tx_http_client,
            batcher_storage_file_handle: batcher_storage_handle,
            state_sync_storage_file_handle: state_sync_storage_handle,
            class_manager_storage_file_handles: class_manager_storage_handles,
            node_config,
            monitoring_client,
            _clients,
        }
    }

    pub async fn assert_add_tx_success(&self, tx: RpcTransaction) -> TransactionHash {
        self.add_tx_http_client.assert_add_tx_success(tx).await
    }
}

pub fn create_consensus_manager_configs_and_channels(
    ports: Vec<u16>,
) -> (
    Vec<ConsensusManagerConfig>,
    BroadcastTopicChannels<StreamMessage<ProposalPart, HeightAndRound>>,
) {
    let mut network_configs = create_connected_network_configs(ports);

    // TODO(Tsabary): Need to also add a channel for votes, in addition to the proposals channel.
    let channels_network_config = network_configs.pop().unwrap();

    let n_network_configs = network_configs.len();
    let consensus_manager_configs =
        create_consensus_manager_configs_from_network_configs(network_configs, n_network_configs);

    let broadcast_channels = network_config_into_broadcast_channels(
        channels_network_config,
        Topic::new(consensus_manager_configs[0].proposals_topic.clone()),
    );

    (consensus_manager_configs, broadcast_channels)
}
