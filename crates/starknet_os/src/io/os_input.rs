use std::collections::HashMap;

use starknet_api::core::ContractAddress;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_types_core::felt::Felt;

#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
pub struct CommitmentInfo {
    _previous_root: HashOutput,
    _updated_root: HashOutput,
    _tree_height: SubTreeHeight,
    // TODO(Dori, 1/8/2025): The value type here should probably be more specific (NodeData<L> for
    //   L: Leaf). This poses a problem in deserialization, as a serialized edge node and a
    //   serialized contract state leaf are both currently vectors of 3 field elements; as the
    //   semantics of the values are unimportant for the OS commitments, we make do with a vector
    //   of field elements as values for now.
    _commitment_facts: HashMap<HashOutput, Vec<Felt>>,
}

/// All input needed to initialize the execution helper.
// TODO(Dori): Add all fields needed to compute commitments, initialize a CachedState and other data
//   required by the execution helper.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
pub struct StarknetOsInput {
    _contract_state_commitment_info: CommitmentInfo,
    _address_to_storage_commitment_info: HashMap<ContractAddress, CommitmentInfo>,
    _contract_class_commitment_info: CommitmentInfo,
}
