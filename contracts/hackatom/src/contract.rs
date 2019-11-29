use serde::{Deserialize, Serialize};
use snafu::{OptionExt, ResultExt};

use cosmwasm::errors::{ContractErr, ParseErr, Result, SerializeErr, Unauthorized};
use cosmwasm::query::{perform_raw_query};
use cosmwasm::serde::{from_slice, to_vec};
use cosmwasm::storage::Storage;
use cosmwasm::types::{CosmosMsg, Params, QueryResponse, RawQuery, Response};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct InitMsg {
    pub verifier: String,
    pub beneficiary: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct State {
    pub verifier: String,
    pub beneficiary: String,
    pub funder: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HandleMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum QueryMsg {
    Raw(RawQuery),
}

pub static CONFIG_KEY: &[u8] = b"config";

pub fn init<T: Storage>(store: &mut T, params: Params, msg: Vec<u8>) -> Result<Response> {
    let msg: InitMsg = from_slice(&msg).context(ParseErr { kind: "InitMsg" })?;
    store.set(
        CONFIG_KEY,
        &to_vec(&State {
            verifier: msg.verifier,
            beneficiary: msg.beneficiary,
            funder: params.message.signer,
        })
        .context(SerializeErr { kind: "State" })?,
    );
    Ok(Response::default())
}

pub fn handle<T: Storage>(store: &mut T, params: Params, _: Vec<u8>) -> Result<Response> {
    let data = store.get(CONFIG_KEY).context(ContractErr {
        msg: "uninitialized data",
    })?;
    let state: State = from_slice(&data).context(ParseErr { kind: "State" })?;

    if params.message.signer == state.verifier {
        let res = Response {
            messages: vec![CosmosMsg::Send {
                from_address: params.contract.address,
                to_address: state.beneficiary,
                amount: params.contract.balance.unwrap_or_default(),
            }],
            log: Some("released funds!".to_string()),
            data: None,
        };
        Ok(res)
    } else {
        Unauthorized {}.fail()
    }
}

pub fn query<T: Storage>(store: &T, msg: Vec<u8>) -> Result<QueryResponse> {
    let msg: QueryMsg = from_slice(&msg).context(ParseErr {kind: "QueryMsg"})?;
    match msg {
        QueryMsg::Raw(raw) => perform_raw_query(store, raw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::from_utf8;
    use cosmwasm::mock::MockStorage;
    use cosmwasm::types::{coin, mock_params, Model};

    #[test]
    fn proper_initialization() {
        let mut store = MockStorage::new();
        let msg = to_vec(&InitMsg {
            verifier: String::from("verifies"),
            beneficiary: String::from("benefits"),
        })
        .unwrap();
        let params = mock_params("creator", &coin("1000", "earth"), &[]);
        let res = init(&mut store, params, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's check the state
        let data = store.get(CONFIG_KEY).expect("no data stored");
        let state: State = from_slice(&data).unwrap();
        assert_eq!(
            state,
            State {
                verifier: "verifies".to_string(),
                beneficiary: "benefits".to_string(),
                funder: "creator".to_string(),
            }
        );
    }

    #[test]
    fn proper_init_and_query() {
        let mut store = MockStorage::new();
        let msg = to_vec(&InitMsg {
            verifier: String::from("foo"),
            beneficiary: String::from("bar"),
        })
            .unwrap();
        let params = mock_params("creator", &coin("1000", "earth"), &[]);
        let _res = init(&mut store, params, msg).unwrap();

        // query for invalid key, no error, return 0 results
        let msg = to_vec(&QueryMsg::Raw(RawQuery{
            key: "random".to_string(),
        })).unwrap();
        let q_res = query(&store, msg).unwrap();
        assert_eq!(q_res.results.len(), 0);

        // query for state
        let model = raw_query(&store, CONFIG_KEY).unwrap();
        let state: State = from_slice(model.val.as_bytes()).unwrap();
        assert_eq!(
            state,
            State {
                verifier: "foo".to_string(),
                beneficiary: "bar".to_string(),
                funder: "creator".to_string(),
            }
        );
    }

    // raw_query is for testing and panic's on error
    pub fn raw_query<T: Storage>(store: &T, bkey: &[u8]) -> Option<Model> {
        let key = from_utf8(bkey).unwrap().to_string();
        let msg = to_vec(&QueryMsg::Raw(RawQuery{key})).unwrap();
        let mut res = query(store, msg).unwrap();
        let model = res.results.pop();
        if let Some(m) = &model {
            assert_eq!(m.key.as_bytes(), bkey);
        }
        model

    }

    #[test]
    fn fails_on_bad_init() {
        let mut store = MockStorage::new();
        let bad_msg = b"{}".to_vec();
        let params = mock_params("creator", &coin("1000", "earth"), &[]);
        let res = init(&mut store, params, bad_msg);
        assert_eq!(true, res.is_err());
    }

    #[test]
    fn proper_handle() {
        let mut store = MockStorage::new();

        // initialize the store
        let init_msg = to_vec(&InitMsg {
            verifier: String::from("verifies"),
            beneficiary: String::from("benefits"),
        })
        .unwrap();
        let init_params = mock_params("creator", &coin("1000", "earth"), &coin("1000", "earth"));
        let init_res = init(&mut store, init_params, init_msg).unwrap();
        assert_eq!(0, init_res.messages.len());

        // beneficiary can release it
        let handle_params = mock_params("verifies", &coin("15", "earth"), &coin("1015", "earth"));
        let handle_res = handle(&mut store, handle_params, Vec::new()).unwrap();
        assert_eq!(1, handle_res.messages.len());
        let msg = handle_res.messages.get(0).expect("no message");
        assert_eq!(
            msg,
            &CosmosMsg::Send {
                from_address: "cosmos2contract".to_string(),
                to_address: "benefits".to_string(),
                amount: coin("1015", "earth"),
            }
        );

        // it worked, let's check the state
        let data = store.get(CONFIG_KEY).expect("no data stored");
        let state: State = from_slice(&data).unwrap();
        assert_eq!(
            state,
            State {
                verifier: "verifies".to_string(),
                beneficiary: "benefits".to_string(),
                funder: "creator".to_string(),
            }
        );
    }

    #[test]
    fn failed_handle() {
        let mut store = MockStorage::new();

        // initialize the store
        let init_msg = to_vec(&InitMsg {
            verifier: String::from("verifies"),
            beneficiary: String::from("benefits"),
        })
        .unwrap();
        let init_params = mock_params("creator", &coin("1000", "earth"), &coin("1000", "earth"));
        let init_res = init(&mut store, init_params, init_msg).unwrap();
        assert_eq!(0, init_res.messages.len());

        // beneficiary can release it
        let handle_params = mock_params("benefits", &[], &coin("1000", "earth"));
        let handle_res = handle(&mut store, handle_params, Vec::new());
        assert!(handle_res.is_err());

        // state should not change
        let data = store.get(CONFIG_KEY).expect("no data stored");
        let state: State = from_slice(&data).unwrap();
        assert_eq!(
            state,
            State {
                verifier: "verifies".to_string(),
                beneficiary: "benefits".to_string(),
                funder: "creator".to_string(),
            }
        );
    }
}
