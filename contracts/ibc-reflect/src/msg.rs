#![allow(clippy::field_reassign_with_default)] // see https://github.com/CosmWasm/cosmwasm/issues/685

use cosmwasm_std::{ContractResult, CosmosMsg, HumanAddr};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// InitMsg just needs to know the code_id of a reflect contract to spawn sub-accounts
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub reflect_code_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    /// InitCallback is returned from reflect contract after a new contract is set up
    InitCallback {
        /// id was provided in the InitMsg
        id: String,
        /// contract_addr is the address of this contract
        contract_addr: HumanAddr,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns (reflect) account that is attached to this channel,
    /// or none.
    Account { channel_id: String },
    /// Returns all (channel, reflect_account) pairs.
    /// No pagination - this is a test contract
    ListAccounts {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AccountResponse {
    pub account: Option<HumanAddr>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ListAccountsResponse {
    pub accounts: Vec<AccountInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AccountInfo {
    pub account: HumanAddr,
    pub channel_id: String,
}

/// This is the message we send to the reflect contract to initialize it
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ReflectInitMsg {
    pub callback_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReflectHandleMsg {
    ReflectMsg { msgs: Vec<CosmosMsg> },
}

/// This is the format of the packets we expect to receive
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PacketMsg {
    pub msgs: Vec<CosmosMsg>,
}

/// This is the format of the packets we send on ack
/// Just acknowledge success or error
pub type AcknowledgementMsg = ContractResult<()>;
