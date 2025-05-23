use apollo_gateway::errors::RPCStateReaderError;
use blockifier::blockifier_versioned_constants::VersionedConstantsError;
use blockifier::state::errors::StateError;
use blockifier::transaction::errors::TransactionExecutionError;
use serde_json::Error as SerdeError;
use starknet_api::StarknetApiError;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(clippy::enum_variant_names)]
pub enum ReexecutionError {
    #[error("Cannot discern chain ID from URL: {0}")]
    AmbiguousChainIdFromUrl(String),
    #[error(transparent)]
    Rpc(#[from] RPCStateReaderError),
    #[error(transparent)]
    Serde(#[from] SerdeError),
    #[error(transparent)]
    StarknetApi(#[from] StarknetApiError),
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    TransactionExecutionError(#[from] TransactionExecutionError),
    #[error(transparent)]
    VersionedConstants(#[from] VersionedConstantsError),
}

pub type ReexecutionResult<T> = Result<T, ReexecutionError>;
