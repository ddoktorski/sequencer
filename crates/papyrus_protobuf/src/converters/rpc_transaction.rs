#[cfg(test)]
#[path = "rpc_transaction_test.rs"]
mod rpc_transaction_test;

use prost::Message;
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
    RpcTransaction,
};
use starknet_api::state::SierraContractClass;
use starknet_api::transaction::fields::{AllResourceBounds, ValidResourceBounds};
use starknet_api::transaction::{
    DeclareTransactionV3,
    DeployAccountTransactionV3,
    InvokeTransactionV3,
};

use super::ProtobufConversionError;
use crate::auto_impl_into_and_try_from_vec_u8;
use crate::mempool::RpcTransactionWrapper;
use crate::protobuf::{self};
use crate::transaction::DeclareTransactionV3Common;
auto_impl_into_and_try_from_vec_u8!(RpcTransactionWrapper, protobuf::MempoolTransaction);

impl TryFrom<protobuf::MempoolTransaction> for RpcTransactionWrapper {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::MempoolTransaction) -> Result<Self, Self::Error> {
        Ok(RpcTransactionWrapper(RpcTransaction::try_from(value)?))
    }
}
impl From<RpcTransactionWrapper> for protobuf::MempoolTransaction {
    fn from(value: RpcTransactionWrapper) -> Self {
        protobuf::MempoolTransaction::from(value.0)
    }
}

impl TryFrom<protobuf::MempoolTransaction> for RpcTransaction {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::MempoolTransaction) -> Result<Self, Self::Error> {
        let txn = value.txn.ok_or(ProtobufConversionError::MissingField {
            field_description: "RpcTransaction::txn",
        })?;
        Ok(match txn {
            protobuf::mempool_transaction::Txn::DeclareV3(txn) => {
                RpcTransaction::Declare(RpcDeclareTransaction::V3(txn.try_into()?))
            }
            protobuf::mempool_transaction::Txn::DeployAccountV3(txn) => {
                RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(txn.try_into()?))
            }
            protobuf::mempool_transaction::Txn::InvokeV3(txn) => {
                RpcTransaction::Invoke(RpcInvokeTransaction::V3(txn.try_into()?))
            }
        })
    }
}

impl From<RpcTransaction> for protobuf::MempoolTransaction {
    fn from(value: RpcTransaction) -> Self {
        match value {
            RpcTransaction::Declare(RpcDeclareTransaction::V3(txn)) => {
                protobuf::MempoolTransaction {
                    txn: Some(protobuf::mempool_transaction::Txn::DeclareV3(txn.into())),
                    // TODO(alonl): Consider removing transaction hash from protobuf
                    transaction_hash: None,
                }
            }
            RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(txn)) => {
                protobuf::MempoolTransaction {
                    txn: Some(protobuf::mempool_transaction::Txn::DeployAccountV3(txn.into())),
                    // TODO(alonl): Consider removing transaction hash from protobuf
                    transaction_hash: None,
                }
            }
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(txn)) => {
                protobuf::MempoolTransaction {
                    txn: Some(protobuf::mempool_transaction::Txn::InvokeV3(txn.into())),
                    // TODO(alonl): Consider removing transaction hash from protobuf
                    transaction_hash: None,
                }
            }
        }
    }
}

impl TryFrom<protobuf::DeployAccountV3> for RpcDeployAccountTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::DeployAccountV3) -> Result<Self, Self::Error> {
        let snapi_deploy_account: DeployAccountTransactionV3 = value.try_into()?;
        // This conversion can fail only if the resource_bounds are not AllResources.
        snapi_deploy_account.try_into().map_err(|_| ProtobufConversionError::MissingField {
            field_description: "resource_bounds",
        })
    }
}

impl From<RpcDeployAccountTransactionV3> for protobuf::DeployAccountV3 {
    fn from(value: RpcDeployAccountTransactionV3) -> Self {
        let snapi_deploy_account: DeployAccountTransactionV3 = value.into();
        snapi_deploy_account.into()
    }
}

impl TryFrom<protobuf::InvokeV3> for RpcInvokeTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::InvokeV3) -> Result<Self, Self::Error> {
        let snapi_invoke: InvokeTransactionV3 = value.try_into()?;
        // This conversion can fail only if the resource_bounds are not AllResources.
        snapi_invoke.try_into().map_err(|_| ProtobufConversionError::MissingField {
            field_description: "resource_bounds",
        })
    }
}

impl From<RpcInvokeTransactionV3> for protobuf::InvokeV3 {
    fn from(value: RpcInvokeTransactionV3) -> Self {
        let snapi_invoke: InvokeTransactionV3 = value.into();
        snapi_invoke.into()
    }
}

impl TryFrom<protobuf::DeclareV3WithClass> for RpcDeclareTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::DeclareV3WithClass) -> Result<Self, Self::Error> {
        let (snapi_declare, class) = value.try_into()?;
        Ok(Self {
            resource_bounds: match snapi_declare.resource_bounds {
                ValidResourceBounds::AllResources(resource_bounds) => resource_bounds,
                _ => {
                    return Err(ProtobufConversionError::MissingField {
                        field_description: "resource_bounds",
                    });
                }
            },
            sender_address: snapi_declare.sender_address,
            signature: snapi_declare.signature,
            nonce: snapi_declare.nonce,
            compiled_class_hash: snapi_declare.compiled_class_hash,
            contract_class: class,
            tip: snapi_declare.tip,
            paymaster_data: snapi_declare.paymaster_data,
            account_deployment_data: snapi_declare.account_deployment_data,
            nonce_data_availability_mode: snapi_declare.nonce_data_availability_mode,
            fee_data_availability_mode: snapi_declare.fee_data_availability_mode,
        })
    }
}

impl From<RpcDeclareTransactionV3> for protobuf::DeclareV3WithClass {
    fn from(value: RpcDeclareTransactionV3) -> Self {
        let snapi_declare = DeclareTransactionV3 {
            resource_bounds: ValidResourceBounds::AllResources(value.resource_bounds),
            sender_address: value.sender_address,
            signature: value.signature,
            nonce: value.nonce,
            compiled_class_hash: value.compiled_class_hash,
            tip: value.tip,
            paymaster_data: value.paymaster_data,
            account_deployment_data: value.account_deployment_data,
            nonce_data_availability_mode: value.nonce_data_availability_mode,
            fee_data_availability_mode: value.fee_data_availability_mode,
            class_hash: value.contract_class.calculate_class_hash(),
        };
        (snapi_declare, value.contract_class).into()
    }
}

impl TryFrom<protobuf::DeclareV3WithClass> for (DeclareTransactionV3, SierraContractClass) {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::DeclareV3WithClass) -> Result<Self, Self::Error> {
        let common = DeclareTransactionV3Common::try_from(value.common.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "DeclareV3WithClass::common",
            },
        )?)?;
        let class: SierraContractClass = SierraContractClass::try_from(value.class.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "DeclareV3WithClass::class",
            },
        )?)?;
        let snapi_declare = DeclareTransactionV3 {
            resource_bounds: common.resource_bounds,
            sender_address: common.sender_address,
            signature: common.signature,
            nonce: common.nonce,
            tip: common.tip,
            paymaster_data: common.paymaster_data,
            account_deployment_data: common.account_deployment_data,
            nonce_data_availability_mode: common.nonce_data_availability_mode,
            fee_data_availability_mode: common.fee_data_availability_mode,
            compiled_class_hash: common.compiled_class_hash,
            class_hash: class.calculate_class_hash(),
        };
        Ok((snapi_declare, class))
    }
}

impl From<(DeclareTransactionV3, SierraContractClass)> for protobuf::DeclareV3WithClass {
    fn from(value: (DeclareTransactionV3, SierraContractClass)) -> Self {
        let common = DeclareTransactionV3Common {
            resource_bounds: value.0.resource_bounds,
            sender_address: value.0.sender_address,
            signature: value.0.signature,
            nonce: value.0.nonce,
            tip: value.0.tip,
            paymaster_data: value.0.paymaster_data,
            account_deployment_data: value.0.account_deployment_data,
            nonce_data_availability_mode: value.0.nonce_data_availability_mode,
            fee_data_availability_mode: value.0.fee_data_availability_mode,
            compiled_class_hash: value.0.compiled_class_hash,
        };
        Self { common: Some(common.into()), class: Some(value.1.into()) }
    }
}

impl TryFrom<protobuf::ResourceBounds> for AllResourceBounds {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ResourceBounds) -> Result<Self, Self::Error> {
        Ok(Self {
            l1_gas: value
                .l1_gas
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "ResourceBounds::l1_gas",
                })?
                .try_into()?,
            l2_gas: value
                .l2_gas
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "ResourceBounds::l2_gas",
                })?
                .try_into()?,
            l1_data_gas: value
                .l1_data_gas
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "ResourceBounds::l1_data_gas",
                })?
                .try_into()?,
        })
    }
}

impl From<AllResourceBounds> for protobuf::ResourceBounds {
    fn from(value: AllResourceBounds) -> Self {
        ValidResourceBounds::AllResources(value).into()
    }
}
