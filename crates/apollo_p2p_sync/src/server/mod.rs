use std::fmt::Debug;

use apollo_class_manager_types::{ClassManagerClientError, SharedClassManagerClient};
use apollo_network::network_manager::{ServerQueryManager, SqmrServerReceiver};
use apollo_protobuf::converters::ProtobufConversionError;
use apollo_protobuf::sync::{
    BlockHashOrNumber,
    ClassQuery,
    ContractDiff,
    DataOrFin,
    DeclaredClass,
    DeprecatedDeclaredClass,
    EventQuery,
    HeaderQuery,
    Query,
    SignedBlockHeader,
    StateDiffChunk,
    StateDiffQuery,
    TransactionQuery,
};
use apollo_storage::body::BodyStorageReader;
use apollo_storage::class_manager::ClassManagerStorageReader;
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::state::StateStorageReader;
use apollo_storage::{db, StorageReader, StorageTxn};
use async_trait::async_trait;
use futures::never::Never;
use futures::StreamExt;
use papyrus_common::pending_classes::ApiContractClass;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::ContractClass;
use starknet_api::core::ClassHash;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::{Event, FullTransaction, TransactionHash};
use tracing::{debug, error, info};

#[cfg(test)]
mod test;

mod utils;

#[derive(thiserror::Error, Debug)]
pub enum P2pSyncServerError {
    #[error(transparent)]
    DBInternalError(#[from] apollo_storage::StorageError),
    #[error("Block number is out of range. Query: {query:?}, counter: {counter}")]
    BlockNumberOutOfRange { query: Query, counter: u64 },
    // TODO(Shahak): add data type to the error message.
    #[error("Block not found. Block: {block_hash_or_number:?}")]
    BlockNotFound { block_hash_or_number: BlockHashOrNumber },
    #[error("Class not found. Class hash: {class_hash}")]
    ClassNotFound { class_hash: ClassHash },
    // This error should be non recoverable.
    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),
    // TODO(Shahak): remove this error, use BlockNotFound instead.
    // This error should be non recoverable.
    #[error("Block {block_number:?} is in the storage but its signature isn't.")]
    SignatureNotFound { block_number: BlockNumber },
    #[error(transparent)]
    SendError(#[from] futures::channel::mpsc::SendError),
    #[error(transparent)]
    ClassManagerClientError(#[from] ClassManagerClientError),
}

impl P2pSyncServerError {
    pub fn should_log_in_error_level(&self) -> bool {
        match self {
            Self::JoinError(_) | Self::SignatureNotFound { .. } | Self::SendError { .. } | Self::ClassManagerClientError { .. }
            // TODO(shahak): Consider returning false for some of the StorageError variants.
            | Self::DBInternalError { .. } => true,
            Self::BlockNumberOutOfRange { .. } | Self::BlockNotFound { .. } | Self::ClassNotFound { .. } => false,
        }
    }
}

type HeaderReceiver = SqmrServerReceiver<HeaderQuery, DataOrFin<SignedBlockHeader>>;
type StateDiffReceiver = SqmrServerReceiver<StateDiffQuery, DataOrFin<StateDiffChunk>>;
type TransactionReceiver = SqmrServerReceiver<TransactionQuery, DataOrFin<FullTransaction>>;
type ClassReceiver = SqmrServerReceiver<ClassQuery, DataOrFin<(ApiContractClass, ClassHash)>>;
type EventReceiver = SqmrServerReceiver<EventQuery, DataOrFin<(Event, TransactionHash)>>;

pub struct P2pSyncServerChannels {
    header_receiver: HeaderReceiver,
    state_diff_receiver: StateDiffReceiver,
    transaction_receiver: TransactionReceiver,
    class_receiver: ClassReceiver,
    event_receiver: EventReceiver,
}

impl P2pSyncServerChannels {
    pub fn new(
        header_receiver: HeaderReceiver,
        state_diff_receiver: StateDiffReceiver,
        transaction_receiver: TransactionReceiver,
        class_receiver: ClassReceiver,
        event_receiver: EventReceiver,
    ) -> Self {
        Self {
            header_receiver,
            state_diff_receiver,
            transaction_receiver,
            class_receiver,
            event_receiver,
        }
    }
}

/// A P2pSyncServer receives inbound queries and returns their corresponding data.
pub struct P2pSyncServer {
    storage_reader: StorageReader,
    p2p_sync_channels: P2pSyncServerChannels,
    class_manager_client: SharedClassManagerClient,
}

impl P2pSyncServer {
    pub async fn run(self) -> Never {
        let P2pSyncServerChannels {
            mut header_receiver,
            mut state_diff_receiver,
            mut transaction_receiver,
            mut class_receiver,
            mut event_receiver,
        } = self.p2p_sync_channels;
        loop {
            tokio::select! {
                maybe_server_query_manager = header_receiver.next() => {
                    let server_query_manager = maybe_server_query_manager.expect(
                        "Header queries sender was unexpectedly dropped."
                    );
                    register_query(self.storage_reader.clone(), server_query_manager, self.class_manager_client.clone(), "header");
                }
                maybe_server_query_manager = state_diff_receiver.next() => {
                    let server_query_manager = maybe_server_query_manager.expect(
                        "State diff queries sender was unexpectedly dropped."
                    );
                    register_query(self.storage_reader.clone(), server_query_manager, self.class_manager_client.clone(), "state diff");
                }
                maybe_server_query_manager = transaction_receiver.next() => {
                    let server_query_manager = maybe_server_query_manager.expect(
                        "Transaction queries sender was unexpectedly dropped."
                    );
                    register_query(self.storage_reader.clone(), server_query_manager, self.class_manager_client.clone(), "transaction");
                }
                maybe_server_query_manager = class_receiver.next() => {
                    let server_query_manager = maybe_server_query_manager.expect(
                        "Class queries sender was unexpectedly dropped."
                    );
                    register_query(self.storage_reader.clone(), server_query_manager, self.class_manager_client.clone(), "class");
                }
                maybe_server_query_manager = event_receiver.next() => {
                    let server_query_manager = maybe_server_query_manager.expect(
                        "Event queries sender was unexpectedly dropped."
                    );
                    register_query(self.storage_reader.clone(), server_query_manager, self.class_manager_client.clone(), "event");
                }
            };
        }
    }

    pub fn new(
        storage_reader: StorageReader,
        p2p_sync_channels: P2pSyncServerChannels,
        class_manager_client: SharedClassManagerClient,
    ) -> Self {
        Self { storage_reader, p2p_sync_channels, class_manager_client }
    }
}
fn register_query<Data, TQuery>(
    storage_reader: StorageReader,
    server_query_manager: ServerQueryManager<TQuery, DataOrFin<Data>>,
    class_manager_client: SharedClassManagerClient,
    protocol_decription: &str,
) where
    Data: FetchBlockData + Send + 'static,
    TQuery: TryFrom<Vec<u8>, Error = ProtobufConversionError> + Send + Clone + Debug + 'static,
    Query: From<TQuery>,
{
    let protocol_decription = protocol_decription.to_owned();
    let query = server_query_manager.query().clone();
    match query {
        Ok(query) => {
            debug!("Sync server received a new inbound query {query:?}");
            tokio::task::spawn(async move {
                let result = send_data_for_query(
                    storage_reader,
                    server_query_manager,
                    class_manager_client,
                    protocol_decription.as_str(),
                )
                .await;
                if let Err(error) = result {
                    if error.should_log_in_error_level() {
                        error!("Running inbound query {query:?} failed on {error:?}");
                    }
                    Err(error)
                } else {
                    Ok(())
                }
            });
        }
        Err(error) => {
            error!("Failed to parse inbound query: {error:?}");
            server_query_manager.report_peer()
        }
    }
}

#[async_trait]
pub trait FetchBlockData: Sized {
    async fn fetch_block_data(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
        class_manager_client: &mut SharedClassManagerClient,
    ) -> Result<Vec<Self>, P2pSyncServerError>;
}

#[async_trait]
impl FetchBlockData for SignedBlockHeader {
    async fn fetch_block_data(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
        _class_manager_client: &mut SharedClassManagerClient,
    ) -> Result<Vec<Self>, P2pSyncServerError> {
        let mut header =
            txn.get_block_header(block_number)?.ok_or(P2pSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            })?;
        // TODO(shahak): Remove this once central sync fills the state_diff_length field.
        if header.state_diff_length.is_none() {
            header.state_diff_length = Some(
                txn.get_state_diff(block_number)?
                    .ok_or(P2pSyncServerError::BlockNotFound {
                        block_hash_or_number: BlockHashOrNumber::Number(block_number),
                    })?
                    .len(),
            );
        }
        let signature = txn
            .get_block_signature(block_number)?
            .ok_or(P2pSyncServerError::SignatureNotFound { block_number })?;
        Ok(vec![SignedBlockHeader { block_header: header, signatures: vec![signature] }])
    }
}

#[async_trait]
impl FetchBlockData for StateDiffChunk {
    async fn fetch_block_data(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
        _class_manager_client: &mut SharedClassManagerClient,
    ) -> Result<Vec<Self>, P2pSyncServerError> {
        let thin_state_diff =
            txn.get_state_diff(block_number)?.ok_or(P2pSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            })?;
        Ok(split_thin_state_diff(thin_state_diff))
    }
}

#[async_trait]
impl FetchBlockData for FullTransaction {
    async fn fetch_block_data(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
        _class_manager_client: &mut SharedClassManagerClient,
    ) -> Result<Vec<Self>, P2pSyncServerError> {
        let transactions =
            txn.get_block_transactions(block_number)?.ok_or(P2pSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            })?;
        let transaction_outputs = txn.get_block_transaction_outputs(block_number)?.ok_or(
            P2pSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            },
        )?;
        let transaction_hashes = txn.get_block_transaction_hashes(block_number)?.ok_or(
            P2pSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            },
        )?;
        let mut result: Vec<FullTransaction> = Vec::new();
        for (transaction, transaction_output, transaction_hash) in transactions
            .into_iter()
            .zip(transaction_outputs.into_iter())
            .zip(transaction_hashes.into_iter())
            .map(|((a, b), c)| (a, b, c))
        {
            result.push(FullTransaction { transaction, transaction_output, transaction_hash });
        }
        Ok(result)
    }
}

#[async_trait]
impl FetchBlockData for (ApiContractClass, ClassHash) {
    async fn fetch_block_data(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
        class_manager_client: &mut SharedClassManagerClient,
    ) -> Result<Vec<Self>, P2pSyncServerError> {
        let thin_state_diff =
            txn.get_state_diff(block_number)?.ok_or(P2pSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            })?;

        if block_number >= txn.get_class_manager_block_marker()? {
            return Err(P2pSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            });
        }

        let declared_classes = thin_state_diff.declared_classes;
        let deprecated_declared_classes = thin_state_diff.deprecated_declared_classes;
        let mut result = Vec::new();
        for class_hash in deprecated_declared_classes {
            let ContractClass::V0(deprecated_contract_class) = class_manager_client
                .get_executable(class_hash)
                .await?
                .ok_or(P2pSyncServerError::ClassNotFound { class_hash })?
            else {
                panic!("Received a cairo1 contract, expected cairo0");
            };
            result.push((
                ApiContractClass::DeprecatedContractClass(deprecated_contract_class),
                class_hash,
            ));
        }

        for (class_hash, _) in declared_classes {
            let sierra = class_manager_client
                .get_sierra(class_hash)
                .await?
                .ok_or(P2pSyncServerError::ClassNotFound { class_hash })?;
            result.push((ApiContractClass::ContractClass(sierra), class_hash));
        }

        Ok(result)
    }
}

#[async_trait]
impl FetchBlockData for (Event, TransactionHash) {
    async fn fetch_block_data(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, db::RO>,
        _class_manager_client: &mut SharedClassManagerClient,
    ) -> Result<Vec<Self>, P2pSyncServerError> {
        let transaction_outputs = txn.get_block_transaction_outputs(block_number)?.ok_or(
            P2pSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            },
        )?;
        let transaction_hashes = txn.get_block_transaction_hashes(block_number)?.ok_or(
            P2pSyncServerError::BlockNotFound {
                block_hash_or_number: BlockHashOrNumber::Number(block_number),
            },
        )?;

        let mut result = Vec::new();
        for (transaction_output, transaction_hash) in
            transaction_outputs.into_iter().zip(transaction_hashes)
        {
            for event in transaction_output.events() {
                result.push((event.clone(), transaction_hash));
            }
        }
        Ok(result)
    }
}

pub fn split_thin_state_diff(thin_state_diff: ThinStateDiff) -> Vec<StateDiffChunk> {
    let mut state_diff_chunks = Vec::new();
    #[cfg(not(test))]
    let mut contract_addresses = std::collections::HashSet::new();
    #[cfg(test)]
    let mut contract_addresses = std::collections::BTreeSet::new();

    contract_addresses.extend(
        thin_state_diff
            .deployed_contracts
            .keys()
            .chain(thin_state_diff.nonces.keys())
            .chain(thin_state_diff.storage_diffs.keys()),
    );
    for contract_address in contract_addresses {
        let class_hash = thin_state_diff.deployed_contracts.get(&contract_address).cloned();
        let storage_diffs =
            thin_state_diff.storage_diffs.get(&contract_address).cloned().unwrap_or_default();
        let nonce = thin_state_diff.nonces.get(&contract_address).cloned();
        state_diff_chunks.push(StateDiffChunk::ContractDiff(ContractDiff {
            contract_address,
            class_hash,
            nonce,
            storage_diffs,
        }));
    }

    for (class_hash, compiled_class_hash) in thin_state_diff.declared_classes {
        state_diff_chunks
            .push(StateDiffChunk::DeclaredClass(DeclaredClass { class_hash, compiled_class_hash }));
    }

    for class_hash in thin_state_diff.deprecated_declared_classes {
        state_diff_chunks
            .push(StateDiffChunk::DeprecatedDeclaredClass(DeprecatedDeclaredClass { class_hash }));
    }
    state_diff_chunks
}

async fn send_data_for_query<Data, TQuery>(
    storage_reader: StorageReader,
    mut server_query_manager: ServerQueryManager<TQuery, DataOrFin<Data>>,
    mut class_manager_client: SharedClassManagerClient,
    protocol_decription: &str,
) -> Result<(), P2pSyncServerError>
where
    Data: FetchBlockData + Send + 'static,
    TQuery: TryFrom<Vec<u8>, Error = ProtobufConversionError> + Clone,
    Query: From<TQuery>,
{
    // If this function fails, we still want to send fin before failing.
    let result = send_data_without_fin_for_query(
        &storage_reader,
        &mut server_query_manager,
        &mut class_manager_client,
        protocol_decription,
    )
    .await;
    debug!("Sending fin message for inbound sync query");
    server_query_manager.send_response(DataOrFin(None)).await?;
    result
}

async fn send_data_without_fin_for_query<Data, TQuery>(
    storage_reader: &StorageReader,
    server_query_manager: &mut ServerQueryManager<TQuery, DataOrFin<Data>>,
    class_manager_client: &mut SharedClassManagerClient,
    protocol_decription: &str,
) -> Result<(), P2pSyncServerError>
where
    Data: FetchBlockData + Send + 'static,
    TQuery: TryFrom<Vec<u8>, Error = ProtobufConversionError> + Clone,
    Query: From<TQuery>,
{
    let query = server_query_manager.query().clone().expect(
        "Query result contains error even though it was previously checked to have no errors",
    );
    let query = Query::from(query);
    let txn = storage_reader.begin_ro_txn()?;
    let start_block_number = match query.start_block {
        BlockHashOrNumber::Number(BlockNumber(num)) => num,
        BlockHashOrNumber::Hash(block_hash) => {
            txn.get_block_number_by_hash(&block_hash)?
                .ok_or(P2pSyncServerError::BlockNotFound {
                    block_hash_or_number: BlockHashOrNumber::Hash(block_hash),
                })?
                .0
        }
    };
    for block_counter in 0..query.limit {
        let block_number =
            BlockNumber(utils::calculate_block_number(&query, start_block_number, block_counter)?);
        let data_vec = Data::fetch_block_data(block_number, &txn, class_manager_client).await?;
        for data in data_vec {
            // TODO(Shahak): consider implement retry mechanism.
            info!(
                "Sending response for inbound {protocol_decription} query for block \
                 {block_number:?}"
            );
            server_query_manager.send_response(DataOrFin(Some(data))).await?;
        }
    }
    Ok(())
}
