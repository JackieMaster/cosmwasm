use cosmwasm_std::{
    attr, entry_point, from_slice, to_binary, CosmosMsg, Deps, DepsMut, Env, HandleResponse,
    HumanAddr, IbcAcknowledgement, IbcBasicResponse, IbcChannel, IbcMsg, IbcOrder, IbcPacket,
    IbcReceiveResponse, InitResponse, MessageInfo, Order, QueryResponse, StdError, StdResult,
};

use crate::msg::{
    AccountInfo, AccountResponse, AcknowledgementMsg, AdminResponse, BalancesResponse,
    DispatchResponse, HandleMsg, InitMsg, ListAccountsResponse, PacketMsg, QueryMsg,
    WhoAmIResponse,
};
use crate::state::{accounts, accounts_read, config, config_read, AccountData, Config};

pub const IBC_VERSION: &str = "ibc-reflect";

// TODO: make configurable?
/// packets live one houe
const PACKET_LIFETIME: u64 = 60 * 60;

#[entry_point]
pub fn init(deps: DepsMut, _env: Env, info: MessageInfo, _msg: InitMsg) -> StdResult<InitResponse> {
    // we store the reflect_id for creating accounts later
    let cfg = Config { admin: info.sender };
    config(deps.storage).save(&cfg)?;

    Ok(InitResponse {
        messages: vec![],
        attributes: vec![attr("action", "init")],
    })
}

#[entry_point]
pub fn handle(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::UpdateAdmin { admin } => handle_update_admin(deps, info, admin),
        HandleMsg::SendMsgs { channel_id, msgs } => {
            handle_send_msgs(deps, env, info, channel_id, msgs)
        }
        HandleMsg::CheckRemoteBalance { channel_id } => {
            handle_check_remote_balance(deps, env, info, channel_id)
        }
        HandleMsg::SendFunds {
            reflect_channel_id,
            transfer_channel_id,
        } => handle_send_funds(deps, env, info, reflect_channel_id, transfer_channel_id),
    }
}

pub fn handle_update_admin(
    deps: DepsMut,
    info: MessageInfo,
    new_admin: HumanAddr,
) -> StdResult<HandleResponse> {
    // auth check
    let mut cfg = config(deps.storage).load()?;
    if info.sender != cfg.admin {
        return Err(StdError::generic_err("Only admin may set new admin"));
    }
    cfg.admin = new_admin;
    config(deps.storage).save(&cfg)?;

    Ok(HandleResponse {
        messages: vec![],
        attributes: vec![
            attr("action", "handle_update_admin"),
            attr("new_admin", cfg.admin),
        ],
        data: None,
    })
}

pub fn handle_send_msgs(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    channel_id: String,
    msgs: Vec<CosmosMsg>,
) -> StdResult<HandleResponse> {
    // auth check
    let cfg = config(deps.storage).load()?;
    if info.sender != cfg.admin {
        return Err(StdError::generic_err("Only admin may send messages"));
    }
    // ensure the channel exists (not found if not registered)
    accounts(deps.storage).load(channel_id.as_bytes())?;

    // construct a packet to send
    let timeout_timestamp = Some(env.block.time + PACKET_LIFETIME);
    let packet = PacketMsg::Dispatch { msgs };
    let msg = IbcMsg::SendPacket {
        channel_id,
        data: to_binary(&packet)?,
        timeout_block: None,
        timeout_timestamp,
    };

    Ok(HandleResponse {
        messages: vec![msg.into()],
        attributes: vec![attr("action", "handle_send_msgs")],
        data: None,
    })
}

pub fn handle_check_remote_balance(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    channel_id: String,
) -> StdResult<HandleResponse> {
    // auth check
    let cfg = config(deps.storage).load()?;
    if info.sender != cfg.admin {
        return Err(StdError::generic_err("Only admin may send messages"));
    }
    // ensure the channel exists (not found if not registered)
    accounts(deps.storage).load(channel_id.as_bytes())?;

    // construct a packet to send
    let timeout_timestamp = Some(env.block.time + PACKET_LIFETIME);
    let packet = PacketMsg::Balances {};
    let msg = IbcMsg::SendPacket {
        channel_id,
        data: to_binary(&packet)?,
        timeout_block: None,
        timeout_timestamp,
    };

    Ok(HandleResponse {
        messages: vec![msg.into()],
        attributes: vec![attr("action", "handle_check_remote_balance")],
        data: None,
    })
}

pub fn handle_send_funds(
    deps: DepsMut,
    env: Env,
    mut info: MessageInfo,
    reflect_channel_id: String,
    transfer_channel_id: String,
) -> StdResult<HandleResponse> {
    // intentionally no auth check

    // require some funds
    if info.sent_funds.is_empty() {
        return Err(StdError::generic_err(
            "you must send the coins you with to ibc transfer",
        ));
    } else if info.sent_funds.len() > 1 {
        return Err(StdError::generic_err("you can only ibc transfer one coin"));
    }
    let amount = info.sent_funds.swap_remove(0);

    // load remote account
    let data = accounts(deps.storage).load(reflect_channel_id.as_bytes())?;
    let remote_addr = match data.remote_addr {
        Some(addr) => addr,
        None => {
            return Err(StdError::generic_err(
                "We don't have the remote address for this channel",
            ))
        }
    };

    // construct a packet to send
    let timeout_timestamp = Some(env.block.time + PACKET_LIFETIME);
    let msg = IbcMsg::Transfer {
        channel_id: transfer_channel_id,
        to_address: remote_addr,
        amount,
        timeout_block: None,
        timeout_timestamp,
    };

    Ok(HandleResponse {
        messages: vec![msg.into()],
        attributes: vec![attr("action", "handle_send_funds")],
        data: None,
    })
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<QueryResponse> {
    match msg {
        QueryMsg::Admin {} => to_binary(&query_admin(deps)?),
        QueryMsg::Account { channel_id } => to_binary(&query_account(deps, channel_id)?),
        QueryMsg::ListAccounts {} => to_binary(&query_list_accounts(deps)?),
    }
}

pub fn query_account(deps: Deps, channel_id: String) -> StdResult<AccountResponse> {
    let account = accounts_read(deps.storage).load(channel_id.as_bytes())?;
    Ok(account.into())
}

pub fn query_list_accounts(deps: Deps) -> StdResult<ListAccountsResponse> {
    let accounts: StdResult<Vec<_>> = accounts_read(deps.storage)
        .range(None, None, Order::Ascending)
        .map(|r| {
            let (k, account) = r?;
            let channel_id = String::from_utf8(k)?;
            Ok(AccountInfo::convert(channel_id, account))
        })
        .collect();
    Ok(ListAccountsResponse {
        accounts: accounts?,
    })
}

pub fn query_admin(deps: Deps) -> StdResult<AdminResponse> {
    let Config { admin } = config_read(deps.storage).load()?;
    Ok(AdminResponse { admin })
}

#[entry_point]
/// enforces ordering and versioing constraints
pub fn ibc_channel_open(_deps: DepsMut, _env: Env, channel: IbcChannel) -> StdResult<()> {
    if channel.order != IbcOrder::Ordered {
        return Err(StdError::generic_err("Only supports ordered channels"));
    }
    if channel.version.as_str() != IBC_VERSION {
        return Err(StdError::generic_err(format!(
            "Must set version to `{}`",
            IBC_VERSION
        )));
    }
    // TODO: do we need to check counterparty version as well?
    // This flow needs to be well documented
    if let Some(counter_version) = channel.counterparty_version {
        if counter_version.as_str() != IBC_VERSION {
            return Err(StdError::generic_err(format!(
                "Counterparty version must be `{}`",
                IBC_VERSION
            )));
        }
    }

    Ok(())
}

#[entry_point]
/// once it's established, we send a WhoAmI message
pub fn ibc_channel_connect(
    deps: DepsMut,
    env: Env,
    channel: IbcChannel,
) -> StdResult<IbcBasicResponse> {
    let channel_id = channel.endpoint.channel_id;

    // create an account holder the channel exists (not found if not registered)
    let data = AccountData::default();
    accounts(deps.storage).save(channel_id.as_bytes(), &data)?;

    // construct a packet to send
    let timeout_timestamp = Some(env.block.time + PACKET_LIFETIME);
    let packet = PacketMsg::WhoAmI {};
    let msg = IbcMsg::SendPacket {
        channel_id: channel_id.clone(),
        data: to_binary(&packet)?,
        timeout_block: None,
        timeout_timestamp,
    };

    Ok(IbcBasicResponse {
        messages: vec![msg.into()],
        attributes: vec![
            attr("action", "ibc_connect"),
            attr("channel_id", channel_id),
        ],
    })
}

#[entry_point]
/// On closed channel, simply delete the account from our local store
pub fn ibc_channel_close(
    deps: DepsMut,
    _env: Env,
    channel: IbcChannel,
) -> StdResult<IbcBasicResponse> {
    // remove the channel
    let channel_id = channel.endpoint.channel_id;
    accounts(deps.storage).remove(channel_id.as_bytes());

    Ok(IbcBasicResponse {
        messages: vec![],
        attributes: vec![attr("action", "ibc_close"), attr("channel_id", channel_id)],
    })
}

#[entry_point]
/// never should be called as the other side never sends packets
pub fn ibc_packet_receive(
    _deps: DepsMut,
    _env: Env,
    _packet: IbcPacket,
) -> StdResult<IbcReceiveResponse> {
    Ok(IbcReceiveResponse {
        acknowledgement: b"{}".into(),
        messages: vec![],
        attributes: vec![attr("action", "ibc_packet_ack")],
    })
}

#[entry_point]
pub fn ibc_packet_ack(
    deps: DepsMut,
    env: Env,
    ack: IbcAcknowledgement,
) -> StdResult<IbcBasicResponse> {
    // which local channel was this packet send from
    let caller = ack.original_packet.src.channel_id;
    // we need to parse the ack based on our request
    let msg: PacketMsg = from_slice(&ack.original_packet.data)?;
    match msg {
        PacketMsg::Dispatch { .. } => {
            let res: AcknowledgementMsg<DispatchResponse> = from_slice(&ack.acknowledgement)?;
            acknowledge_dispatch(deps, caller, res)
        }
        PacketMsg::WhoAmI {} => {
            let res: AcknowledgementMsg<WhoAmIResponse> = from_slice(&ack.acknowledgement)?;
            acknowledge_who_am_i(deps, caller, res)
        }
        PacketMsg::Balances {} => {
            let res: AcknowledgementMsg<BalancesResponse> = from_slice(&ack.acknowledgement)?;
            acknowledge_balances(deps, env, caller, res)
        }
    }
}

// receive PacketMsg::Dispatch response
fn acknowledge_dispatch(
    _deps: DepsMut,
    _caller: String,
    _ack: AcknowledgementMsg<DispatchResponse>,
) -> StdResult<IbcBasicResponse> {
    // TODO: actually handle success/error?
    Ok(IbcBasicResponse {
        messages: vec![],
        attributes: vec![attr("action", "acknowledge_dispatch")],
    })
}

// receive PacketMsg::WhoAmI response
// store address info in accounts info
fn acknowledge_who_am_i(
    deps: DepsMut,
    caller: String,
    ack: AcknowledgementMsg<WhoAmIResponse>,
) -> StdResult<IbcBasicResponse> {
    // ignore errors (but mention in log)
    let res: WhoAmIResponse = match ack {
        AcknowledgementMsg::Ok(res) => res,
        AcknowledgementMsg::Err(e) => {
            return Ok(IbcBasicResponse {
                messages: vec![],
                attributes: vec![attr("action", "acknowledge_who_am_i"), attr("error", e)],
            })
        }
    };

    accounts(deps.storage).update(caller.as_bytes(), |acct| -> StdResult<_> {
        match acct {
            Some(mut acct) => {
                // set the account the first time
                if acct.remote_addr.is_none() {
                    acct.remote_addr = Some(res.account);
                }
                Ok(acct)
            }
            None => Err(StdError::generic_err("no account to update")),
        }
    })?;

    Ok(IbcBasicResponse {
        messages: vec![],
        attributes: vec![attr("action", "acknowledge_who_am_i")],
    })
}

// receive PacketMsg::Balances response
fn acknowledge_balances(
    deps: DepsMut,
    env: Env,
    caller: String,
    ack: AcknowledgementMsg<BalancesResponse>,
) -> StdResult<IbcBasicResponse> {
    // ignore errors (but mention in log)
    let res: BalancesResponse = match ack {
        AcknowledgementMsg::Ok(res) => res,
        AcknowledgementMsg::Err(e) => {
            return Ok(IbcBasicResponse {
                messages: vec![],
                attributes: vec![attr("action", "acknowledge_balances"), attr("error", e)],
            })
        }
    };

    accounts(deps.storage).update(caller.as_bytes(), |acct| -> StdResult<_> {
        match acct {
            Some(acct) => {
                if let Some(old_addr) = &acct.remote_addr {
                    if old_addr != &res.account {
                        return Err(StdError::generic_err(format!(
                            "remote account changed from {} to {}",
                            old_addr, &res.account
                        )));
                    }
                }
                Ok(AccountData {
                    last_update_time: env.block.time,
                    remote_addr: Some(res.account),
                    remote_balance: res.balances,
                })
            }
            None => Err(StdError::generic_err("no account to update")),
        }
    })?;

    Ok(IbcBasicResponse {
        messages: vec![],
        attributes: vec![attr("action", "acknowledge_balances")],
    })
}

#[entry_point]
/// we just ignore these now. shall we store some info?
pub fn ibc_packet_timeout(
    _deps: DepsMut,
    _env: Env,
    _packet: IbcPacket,
) -> StdResult<IbcBasicResponse> {
    Ok(IbcBasicResponse {
        messages: vec![],
        attributes: vec![attr("action", "ibc_packet_timeout")],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_ibc_channel, mock_ibc_packet_ack, mock_info, MockApi,
        MockQuerier, MockStorage,
    };
    use cosmwasm_std::OwnedDeps;

    const CREATOR: &str = "creator";

    fn setup() -> OwnedDeps<MockStorage, MockApi, MockQuerier> {
        let mut deps = mock_dependencies(&[]);
        let msg = InitMsg {};
        let info = mock_info(CREATOR, &[]);
        let res = init(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        deps
    }

    // connect will run through the entire handshake to set up a proper connect and
    // save the account (tested in detail in `proper_handshake_flow`)
    fn connect(mut deps: DepsMut, channel_id: &str) {
        // open packet has no counterparty version, connect does
        let mut handshake_open = mock_ibc_channel(channel_id, IbcOrder::Ordered, IBC_VERSION);
        handshake_open.counterparty_version = None;
        // first we try to open with a valid handshake
        ibc_channel_open(deps.branch(), mock_env(), handshake_open).unwrap();

        // then we connect (with counter-party version set)
        let handshake_connect = mock_ibc_channel(channel_id, IbcOrder::Ordered, IBC_VERSION);
        let res = ibc_channel_connect(deps.branch(), mock_env(), handshake_connect).unwrap();

        // this should send a WhoAmI request, which is received some blocks later
        assert_eq!(1, res.messages.len());
        match &res.messages[0] {
            CosmosMsg::Ibc(IbcMsg::SendPacket {
                channel_id: packet_channel,
                ..
            }) => assert_eq!(packet_channel.as_str(), channel_id),
            o => panic!("Unexpected message: {:?}", o),
        };
    }

    fn who_am_i_response<T: Into<HumanAddr>>(deps: DepsMut, channel_id: &str, account: T) {
        let packet = PacketMsg::WhoAmI {};
        let response = AcknowledgementMsg::Ok(WhoAmIResponse {
            account: account.into(),
        });
        let ack = IbcAcknowledgement {
            acknowledgement: to_binary(&response).unwrap(),
            original_packet: mock_ibc_packet_ack(channel_id, &packet).unwrap(),
        };
        let res = ibc_packet_ack(deps, mock_env(), ack).unwrap();
        assert_eq!(0, res.messages.len());
    }

    #[test]
    fn init_works() {
        let deps = setup();
        let admin = query_admin(deps.as_ref()).unwrap();
        assert_eq!(CREATOR, admin.admin.as_str());
    }

    #[test]
    fn enforce_version_in_handshake() {
        let mut deps = setup();

        let wrong_order = mock_ibc_channel("channel-12", IbcOrder::Unordered, IBC_VERSION);
        ibc_channel_open(deps.as_mut(), mock_env(), wrong_order).unwrap_err();

        let wrong_version = mock_ibc_channel("channel-12", IbcOrder::Ordered, "reflect");
        ibc_channel_open(deps.as_mut(), mock_env(), wrong_version).unwrap_err();

        let valid_handshake = mock_ibc_channel("channel-12", IbcOrder::Ordered, IBC_VERSION);
        ibc_channel_open(deps.as_mut(), mock_env(), valid_handshake).unwrap();
    }

    #[test]
    fn proper_handshake_flow() {
        // setup and connect handshake
        let mut deps = setup();
        let channel_id = "channel-1234";
        connect(deps.as_mut(), channel_id);

        // check for empty account
        let acct = query_account(deps.as_ref(), channel_id.into()).unwrap();
        assert!(acct.remote_addr.is_none());
        assert!(acct.remote_balance.is_empty());
        assert_eq!(0, acct.last_update_time);

        // now get feedback from WhoAmI packet
        let remote_addr = "account-789";
        who_am_i_response(deps.as_mut(), channel_id, remote_addr);

        // account should be set up
        let acct = query_account(deps.as_ref(), channel_id.into()).unwrap();
        assert_eq!(acct.remote_addr.unwrap(), HumanAddr::from(remote_addr));
        assert!(acct.remote_balance.is_empty());
        assert_eq!(0, acct.last_update_time);
    }

    // #[test]
    // fn handle_dispatch_packet() {
    //     let mut deps = setup();
    //
    //     let channel_id = "channel-123";
    //     let account = "acct-123";
    //
    //     // receive a packet for an unregistered channel returns app-level error (not Result::Err)
    //     let msgs_to_dispatch = vec![BankMsg::Send {
    //         to_address: "my-friend".into(),
    //         amount: coins(123456789, "uatom"),
    //     }
    //     .into()];
    //     let ibc_msg = PacketMsg::Dispatch {
    //         msgs: msgs_to_dispatch.clone(),
    //     };
    //     let packet = mock_ibc_packet_recv(channel_id, &ibc_msg).unwrap();
    //     let res = ibc_packet_receive(deps.as_mut(), mock_env(), packet.clone()).unwrap();
    //     // we didn't dispatch anything
    //     assert_eq!(0, res.messages.len());
    //     // acknowledgement is an error
    //     let ack: AcknowledgementMsg<DispatchResponse> = from_slice(&res.acknowledgement).unwrap();
    //     assert_eq!(
    //         ack.unwrap_err(),
    //         "invalid packet: cosmwasm_std::addresses::HumanAddr not found"
    //     );
    //
    //     // register the channel
    //     connect(deps.as_mut(), channel_id, account);
    //
    //     // receive a packet for an unregistered channel returns app-level error (not Result::Err)
    //     let packet = mock_ibc_packet_recv(channel_id, &ibc_msg).unwrap();
    //     let res = ibc_packet_receive(deps.as_mut(), mock_env(), packet.clone()).unwrap();
    //
    //     // assert app-level success
    //     let ack: AcknowledgementMsg<()> = from_slice(&res.acknowledgement).unwrap();
    //     ack.unwrap();
    //
    //     // and we dispatch the BankMsg
    //     assert_eq!(1, res.messages.len());
    //     // parse the output, ensuring it matches
    //     if let CosmosMsg::Wasm(WasmMsg::Execute {
    //         contract_addr,
    //         msg,
    //         send,
    //     }) = &res.messages[0]
    //     {
    //         assert_eq!(account, contract_addr.as_str());
    //         assert_eq!(0, send.len());
    //         // parse the message - should callback with proper channel_id
    //         let rmsg: ReflectHandleMsg = from_slice(&msg).unwrap();
    //         assert_eq!(
    //             rmsg,
    //             ReflectHandleMsg::ReflectMsg {
    //                 msgs: msgs_to_dispatch
    //             }
    //         );
    //     } else {
    //         panic!("invalid return message: {:?}", res.messages[0]);
    //     }
    //
    //     // invalid packet format on registered channel also returns app-level error
    //     let bad_data = InitMsg {
    //         reflect_code_id: 12345,
    //     };
    //     let packet = mock_ibc_packet_recv(channel_id, &bad_data).unwrap();
    //     let res = ibc_packet_receive(deps.as_mut(), mock_env(), packet.clone()).unwrap();
    //     // we didn't dispatch anything
    //     assert_eq!(0, res.messages.len());
    //     // acknowledgement is an error
    //     let ack: AcknowledgementMsg<DispatchResponse> = from_slice(&res.acknowledgement).unwrap();
    //     assert_eq!(ack.unwrap_err(), "invalid packet: Error parsing into type ibc_reflect_send::msg::PacketMsg: unknown variant `reflect_code_id`, expected one of `dispatch`, `who_am_i`, `balances`");
    // }
}
