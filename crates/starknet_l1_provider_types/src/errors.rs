use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_sequencer_infra::component_client::ClientError;
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum L1ProviderError {
    #[error("Could not initialize the provider: {0}")]
    InitializationError(String),
    #[error("`get_txs` while in `Validate` state")]
    GetTransactionConsensusBug,
    // This is likely due to a crash, restart block proposal.
    #[error("`get_txs` called when provider is not in proposer state")]
    OutOfSessionGetTransactions,
    // This is likely due to a crash, restart block proposal.
    #[error("`validate` called when provider is not in proposer state")]
    OutOfSessionValidate,
    #[error("Unexpected height: expected {expected}, got {got}")]
    UnexpectedHeight { expected: BlockNumber, got: BlockNumber },
    #[error("Cannot transition from {from} to {to}")]
    UnexpectedProviderStateTransition { from: String, to: String },
    #[error("`validate` called while in `Propose` state")]
    ValidateTransactionConsensusBug,
}

impl L1ProviderError {
    pub fn unexpected_transition(from: impl ToString, to: impl ToString) -> Self {
        Self::UnexpectedProviderStateTransition { from: from.to_string(), to: to.to_string() }
    }
}

#[derive(Clone, Debug, Error)]
pub enum L1ProviderClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    L1ProviderError(#[from] L1ProviderError),
}
