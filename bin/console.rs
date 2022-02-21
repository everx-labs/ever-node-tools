/*
* Copyright (C) 2019-2023 EverX. All Rights Reserved.
*
* Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
* this file except in compliance with the License.
*
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific TON DEV software governing permissions and
* limitations under the License.
*/

use adnl::{
    common::TaggedTlObject, client::{AdnlClient, AdnlClientConfig, AdnlClientConfigJson}
};
use std::{convert::TryInto, env, str::FromStr, time::Duration};
use std::collections::HashMap;
use num_bigint::BigUint;
use ton_abi::TokenValue;
use ton_abi::Token;
use ton_abi::Contract;
use ton_abi::Uint;
use ton_api::{
    serialize_boxed,
    ton::{
        self, TLObject, 
        accountaddress::AccountAddress, engine::validator::{ControlQueryError, onestat::OneStat}, 
        raw::ShardAccountState, rpc::engine::validator::ControlQuery
    }
};
#[cfg(feature = "telemetry")]
use ton_api::tag_from_bare_object;
use ton_block::{
    AccountStatus, Deserializable, BlockIdExt, Serializable, ShardAccount
};
use ton_types::{
    error, fail, Result, BuilderData, Ed25519KeyOption, SliceData, UInt256, write_boc
};

include!("../common/src/test.rs");

const ELECTOR_ABI: &[u8] = include_bytes!("Elector.abi.json"); //elector's ABI
const ELECTOR_PROCESS_NEW_STAKE_FUNC_NAME: &str = "process_new_stake"; //elector process_new_stake function name

trait SendReceive {
    fn send<Q: ToString>(params: impl Iterator<Item = Q>) -> Result<TLObject>;
    fn receive<Q: ToString>(
        answer: TLObject, 
        _params: impl Iterator<Item = Q>
    ) -> Result<(String, Vec<u8>)> {
        downcast::<ton_api::ton::engine::validator::Success>(answer)?;
        Ok(("success".to_string(), vec![]))
    }
}

trait ConsoleCommand: SendReceive {
    fn name() -> &'static str;
    fn help() -> &'static str;
}

macro_rules! commands {
    ($($command: ident, $name: literal, $help: literal)*) => {
        $(
            struct $command;
            impl ConsoleCommand for $command {
                fn name() -> &'static str {$name}
                fn help() -> &'static str {$help}
            }
        )*
        fn _command_help(name: &str) -> Result<&str> {
            match name {
                $($name => Ok($command::help()), )*
                _ => fail!("command {} not supported", name)
            }
        }
        fn command_send<Q: ToString>(name: &str, params: impl Iterator<Item = Q>) -> Result<TLObject> {
            match name {
                $($name => $command::send(params), )*
                _ => fail!("command {} not supported", name)
            }
        }
        fn command_receive<Q: ToString>(
            name: &str,
            answer: TLObject,
            params: impl Iterator<Item = Q>
        ) -> Result<(String, Vec<u8>)> {
            match name {
                $($name => $command::receive(answer, params), )*
                _ => fail!("an error occured while receiving a response (command: {})", name)
            }
        }
    };
}

commands! {
    AddAdnlAddr, "addadnl", "addadnl <keyhash> <category>\tuse key as ADNL addr"
    AddValidatorAdnlAddr, "addvalidatoraddr", "addvalidatoraddr <permkeyhash> <keyhash> <expireat>\tadd validator ADNL addr"
    AddValidatorPermKey, "addpermkey", "addpermkey <keyhash> <election-date> <expire-at>\tadd validator permanent key"
    AddValidatorTempKey, "addtempkey", "addtempkey <permkeyhash> <keyhash> <expire-at>\tadd validator temp key"
    AddValidatorBlsKey, "addblskey", "addblskey <permkeyhash> <keyhash> <expire-at>\t add validator bls key"
    Bundle, "bundle", "bundle <block_id>\tprepare bundle"
    ExportPub, "exportpub", "exportpub <keyhash>\texports public key by key hash"
    FutureBundle, "future_bundle", "future_bundle <block_id>\tprepare future bundle"
    GetAccount, "getaccount", "getaccount <account id> <Option<file name>>\tget account info"
    GetAccountState, "getaccountstate", "getaccountstate <account id> <file name>\tsave accountstate to file"
    GetBlockchainConfig, "getblockchainconfig", "getblockchainconfig\tget current config from masterchain state"
    GetConfig, "getconfig", "getconfig <param_number>\tget current config param from masterchain state"
    GetSessionStats, "getconsensusstats", "getconsensusstats\tget consensus statistics for the node"
    GetSelectedStats, "getstatsnew", "getstatsnew\tget status full node or validator in new format"
    GetStats, "getstats", "getstats\tget status full node or validator"
    NewKeypair, "newkey", "newkey\tgenerates new key pair on server"
    SendMessage, "sendmessage", "sendmessage <filename>\tload a serialized message from <filename> and send it to server"
    SetStatesGcInterval, "setstatesgcinterval", "setstatesgcinterval <milliseconds>\tset interval in <milliseconds> between shard states GC runs"
    Sign, "sign", "sign <keyhash> <data>\tsigns bytestring with privkey"
}

fn parse_any<A, Q: ToString>(param_opt: Option<Q>, name: &str, parse_value: impl FnOnce(&str) -> Result<A>) -> Result<A> {
    param_opt
        .ok_or_else(|| error!("insufficient parameters"))
        .and_then(|value| parse_value(value.to_string().trim_matches('\"')))
        .map_err(|err| error!("you must give {}: {}", name, err))
}

fn downcast<T: ton_api::AnyBoxedSerialize>(data: TLObject) -> Result<T> {
    match data.downcast::<T>() {
        Ok(result) => Ok(result),
        Err(obj) => fail!("Wrong downcast {:?} to {}", obj, std::any::type_name::<T>())
    }
}

fn parse_data<Q: ToString>(param_opt: Option<Q>, name: &str) -> Result<ton::bytes> {
    parse_any(
        param_opt,
        &format!("{} in hex format", name),
        |value| Ok(ton::bytes(hex::decode(value)?))
    )
}

fn parse_int256<Q: ToString>(param_opt: Option<Q>, name: &str) -> Result<UInt256> {
    parse_any(
        param_opt,
        &format!("{} in hex or base64 format", name),
        |value| {
            let value = match value.len() {
                44 => base64::decode(value)?,
                64 => hex::decode(value)?,
                length => fail!("wrong hash: {} with length: {}", value, length)
            };
            Ok(UInt256::with_array(value.as_slice().try_into()?))
        }
    )
}

fn parse_int<Q: ToString>(param_opt: Option<Q>, name: &str) -> Result<ton::int> {
    parse_any(param_opt, name, |value| Ok(ton::int::from_str(value)?))
}

fn parse_blockid<Q: ToString>(param_opt: Option<Q>, name: &str) -> Result<BlockIdExt> {
    parse_any(param_opt, name, |value| BlockIdExt::from_str(value))
}

fn now() -> ton::int {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as ton::int
}

fn stats_to_json<'a>(stats: impl IntoIterator<Item = &'a OneStat>) -> serde_json::Value {
    let map = stats.into_iter().map(|stat| {
        let value = if stat.value.is_empty() {
            "null".into()
        } else if let Ok(value) = stat.value.parse::<i64>() {
            value.into()
        } else if let Ok(value) = serde_json::from_str(&stat.value) {
            value
        } else {
            stat.value.trim_matches('\"').into()
        };
        (stat.key.clone(), value)
    }).collect::<serde_json::Map<_, _>>();
    map.into()
}

impl SendReceive for GetStats {
    fn send<Q: ToString>(_params: impl Iterator<Item = Q>) -> Result<TLObject> {
        Ok(TLObject::new(ton::rpc::engine::validator::GetStats))
    }
    fn receive<Q: ToString>(
        answer: TLObject, 
        mut _params: impl Iterator<Item = Q>
    ) -> Result<(String, Vec<u8>)> {
        let data = serialize_boxed(&answer)?;
        let stats = downcast::<ton_api::ton::engine::validator::Stats>(answer)?;
        let description = stats_to_json(stats.stats().iter());
        let description = format!("{:#}", description);
        Ok((description, data))
    }
}

impl SendReceive for GetSelectedStats {
    fn send<Q: ToString>(_params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let req = ton::rpc::engine::validator::GetSelectedStats {
            filter: "*".to_string()
        };
        Ok(TLObject::new(req))
    }
    fn receive<Q: ToString>(
        answer: TLObject, 
        mut _params: impl Iterator<Item = Q>
    ) -> Result<(String, Vec<u8>)> {
        let data = serialize_boxed(&answer)?;
        let stats = downcast::<ton_api::ton::engine::validator::Stats>(answer)?;
        let description = stats_to_json(stats.stats().iter());
        let description = format!("{:#}", description);
        Ok((description, data))
    }
}

impl SendReceive for GetSessionStats {
    fn send<Q: ToString>(_params: impl Iterator<Item = Q>) -> Result<TLObject> {
        Ok(TLObject::new(ton::rpc::engine::validator::GetSessionStats))
    }
    fn receive<Q: ToString>(
        answer: TLObject, 
        mut _params: impl Iterator<Item = Q>
    ) -> Result<(String, Vec<u8>)> {
        let data = serialize_boxed(&answer)?;
        let stats = downcast::<ton_api::ton::engine::validator::SessionStats>(answer)?;
        let description = stats.stats().iter().map(|session_stat| {
            (session_stat.session_id.clone(), stats_to_json(session_stat.stats.iter()))
        }).collect::<serde_json::Map<_, _>>();
        let description = format!("{:#}", serde_json::Value::from(description));
        Ok((description, data))
    }
}

impl SendReceive for NewKeypair {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        match params.next() {
            None => Ok(TLObject::new(ton::rpc::engine::validator::GenerateKeyPair)),
            Some(param) => {
                let mut param = param.to_string();
                param.make_ascii_lowercase();
                match param.as_ref() {
                    "bls" => Ok(TLObject::new(ton::rpc::engine::validator::GenerateBlsKeyPair)),
                    _ => fail!("invalid parameters!")
                }
            },
        }
    }
    fn receive<Q: ToString>(
        answer: TLObject, 
        mut _params: impl Iterator<Item = Q>
    ) -> Result<(String, Vec<u8>)> {
        let answer = downcast::<ton_api::ton::engine::validator::KeyHash>(answer)?;
        let key_hash = answer.key_hash().as_slice().to_vec();
        Ok((format!("received public key hash: {} {}", 
            hex::encode(&key_hash), base64::encode(&key_hash)), key_hash
        ))
    }
}

impl SendReceive for ExportPub {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let key_hash = parse_int256(params.next(), "key_hash")?;
        Ok(TLObject::new(ton::rpc::engine::validator::ExportPublicKey {
            key_hash
        }))
    }
    fn receive<Q: ToString>(
        answer: TLObject, 
        mut _params: impl Iterator<Item = Q>
    ) -> Result<(String, Vec<u8>)> {
        let answer = downcast::<ton_api::ton::PublicKey>(answer)?;
        let pub_key = match answer.key() {
            Some(key) => key.clone().into_vec(),
            None => {
                answer.bls_key()
                    .ok_or_else(|| error!("Public key not found in answer!"))?
                    .0.clone()
            }
        };
        Ok((format!("imported key: {} {}", hex::encode(&pub_key), base64::encode(&pub_key)), pub_key))
    }
}

impl SendReceive for Sign {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let key_hash = parse_int256(params.next(), "key_hash")?;
        let data = parse_data(params.next(), "data")?;
        Ok(TLObject::new(ton::rpc::engine::validator::Sign {
            key_hash,
            data
        }))
    }
    fn receive<Q: ToString>(
        answer: TLObject, 
        mut _params: impl Iterator<Item = Q>
    ) -> Result<(String, Vec<u8>)> {
        let answer = downcast::<ton_api::ton::engine::validator::Signature>(answer)?;
        let signature = answer.signature().0.clone();
        Ok((format!("got signature: {} {}", hex::encode(&signature), base64::encode(&signature)), signature))
    }
}

impl SendReceive for AddValidatorPermKey {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let key_hash =  parse_int256(params.next(), "key_hash")?;
        let election_date = parse_int(params.next(), "election_date")?;
        let ttl = parse_int(params.next(), "expire_at")? - election_date;
        Ok(TLObject::new(ton::rpc::engine::validator::AddValidatorPermanentKey {
            key_hash,
            election_date,
            ttl
        }))
    }
}

impl SendReceive for AddValidatorBlsKey {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let permanent_key_hash = parse_int256(params.next(), "permanent_key_hash")?;
        let key_hash =  parse_int256(params.next(), "key_hash")?;
        let ttl = parse_int(params.next(), "expire_at")? - now();
        Ok(TLObject::new(ton::rpc::engine::validator::AddValidatorBlsKey {
            permanent_key_hash,
            key_hash,
            ttl
        }))
    }
}

impl SendReceive for AddValidatorTempKey {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let permanent_key_hash = parse_int256(params.next(), "permanent_key_hash")?;
        let key_hash = parse_int256(params.next(), "key_hash")?;
        let ttl = parse_int(params.next(), "expire_at")? - now();
        Ok(TLObject::new(ton::rpc::engine::validator::AddValidatorTempKey {
            permanent_key_hash,
            key_hash,
            ttl
        }))
    }
}

impl SendReceive for AddValidatorAdnlAddr {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let permanent_key_hash = parse_int256(params.next(), "permanent_key_hash")?;
        let key_hash = parse_int256(params.next(), "key_hash")?;
        let ttl = parse_int(params.next(), "expire_at")? - now();
        Ok(TLObject::new(ton::rpc::engine::validator::AddValidatorAdnlAddress {
            permanent_key_hash,
            key_hash,
            ttl
        }))
    }
}

impl SendReceive for AddAdnlAddr {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let key_hash = parse_int256(params.next(), "key_hash")?;
        let category = parse_int(params.next(), "category")?;
        if category < 0 || category > 15 {
            fail!("category must be not negative and less than 16")
        }
        Ok(TLObject::new(ton::rpc::engine::validator::AddAdnlId {
            key_hash,
            category
        }))
    }
}

impl SendReceive for Bundle {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let block_id = parse_blockid(params.next(), "block_id")?;
        Ok(TLObject::new(ton::rpc::engine::validator::GetBundle {
            block_id
        }))
    }
}

impl SendReceive for FutureBundle {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let mut prev_block_ids = vec![parse_blockid(params.next(), "block_id")?];
        if let Ok(block_id) = parse_blockid(params.next(), "block_id") {
            prev_block_ids.push(block_id);
        }
        Ok(TLObject::new(ton::rpc::engine::validator::GetFutureBundle {
            prev_block_ids: prev_block_ids.into()
        }))
    }
}

impl SendReceive for SendMessage {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let filename = params.next().ok_or_else(|| error!("insufficient parameters"))?.to_string();
        let body = std::fs::read(&filename)
            .map_err(|e| error!("Can't read file {} with message: {}", filename, e))?;
        Ok(TLObject::new(ton::rpc::lite_server::SendMessage {body: body.into()}))
    }
}

impl SendReceive for GetBlockchainConfig {
    fn send<Q: ToString>(_params: impl Iterator<Item = Q>) -> Result<TLObject> {
        Ok(TLObject::new(ton::rpc::lite_server::GetConfigAll {
            mode: 0,
            id: BlockIdExt::default()
        }))
    }
    fn receive<Q: ToString>(
        answer: TLObject, 
        mut _params: impl Iterator<Item = Q>
    ) -> Result<(String, Vec<u8>)> {
        let config_info = downcast::<ton_api::ton::lite_server::ConfigInfo>(answer)?;

        // We use config_proof because we use standard struct ConfigInfo from ton-tl and
        // ConfigInfo doesn`t contain more suitable fields
        let config_param = hex::encode(config_info.config_proof().0.clone());
        Ok((format!("{}", config_param), config_info.config_proof().0.clone()))
    }
}

impl SendReceive for GetConfig {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let param_number = parse_int(params.next(), "paramnumber")?;
        let mut params: ton::vector<ton::Bare, ton::int> = ton::vector::default();
        params.0.push(param_number);
        Ok(TLObject::new(ton::rpc::lite_server::GetConfigParams {
            mode: 0,
            id: BlockIdExt::default(),
            param_list: params
        }))
    }
    fn receive<Q: ToString>(
        answer: TLObject, 
        mut _params: impl Iterator<Item = Q>
    ) -> Result<(String, Vec<u8>)> {
        let config_info = downcast::<ton_api::ton::lite_server::ConfigInfo>(answer)?;
        let config_param = String::from_utf8(config_info.config_proof().0.clone())?;
        Ok((config_param.to_string(), config_info.config_proof().0.clone()))
    }
}

impl SendReceive for GetAccount {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let account = AccountAddress { 
            account_address: params.next().ok_or_else(|| error!("insufficient parameters"))?.to_string()
        };
        Ok(TLObject::new(ton::rpc::raw::GetShardAccountState {account_address: account}))
    }

    fn receive<Q: ToString>(
        answer: TLObject, 
        mut params: impl Iterator<Item = Q>
    ) -> Result<(String, Vec<u8>)> {
        let shard_account_state = downcast::<ShardAccountState>(answer)?;
        let mut account_info = String::from("{");
        account_info.push_str("\n\"");
        account_info.push_str("acc_type\":\t\"");

        match shard_account_state {
            ShardAccountState::Raw_ShardAccountNone => {
                account_info.push_str(&"Nonexist");
            },
            ShardAccountState::Raw_ShardAccountState(account_state) => {
                let shard_account = ShardAccount::construct_from_bytes(&account_state.shard_account)?;
                let account = shard_account.read_account()?;

                let account_type = match account.status() {
                    AccountStatus::AccStateUninit => "Uninit",
                    AccountStatus::AccStateFrozen => "Frozen",
                    AccountStatus::AccStateActive => "Active",
                    AccountStatus::AccStateNonexist => "Nonexist"
                };
                let balance = account.balance().map_or(0, |val| val.grams.as_u128());
                account_info.push_str(&account_type);
                account_info.push_str("\",\n\"");
                account_info.push_str("balance\":\t");
                account_info.push_str(&balance.to_string());
                account_info.push_str(",\n\"");
                account_info.push_str("last_paid\":\t");
                account_info.push_str(&account.last_paid().to_string());
                account_info.push_str(",\n\"");
                account_info.push_str("last_trans_lt\":\t\"");
                account_info.push_str(&format!("{:#x}", shard_account.last_trans_lt()));
                account_info.push_str("\",\n\"");
                account_info.push_str("data(boc)\":\t\"");
                account_info.push_str(
                    &hex::encode(&write_boc(&shard_account.account_cell())?)
                );
            }
        }
        account_info.push_str("\"\n}");

        params.next();
        let account_data = account_info.as_bytes().to_vec();
        if let Some(boc_name) = params.next() {
            std::fs::write(boc_name.to_string(), &account_data)
                .map_err(|err| error!("Can`t create file: {}", err))?;
        }

        Ok((account_info, account_data))
    }
}

impl SendReceive for GetAccountState {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let account_address = params.next().ok_or_else(|| error!("insufficient parameters"))?.to_string();
        let account_address = AccountAddress { account_address };
        Ok(TLObject::new(ton::rpc::raw::GetShardAccountState {account_address}))
    }

    fn receive<Q: ToString>(
        answer: TLObject, 
        mut params: impl Iterator<Item = Q>
    ) -> Result<(String, Vec<u8>)> {
        let shard_account_state = downcast::<ShardAccountState>(answer)?;

        params.next();
        let boc_name = params
            .next()
            .ok_or_else(|| error!("bad params (boc name not found)!"))?
            .to_string();

        let shard_account_state = shard_account_state
            .shard_account()
            .ok_or_else(|| error!("account not found!"))?;
        
        let shard_account = ShardAccount::construct_from_bytes(&shard_account_state)?;
        let account_state = write_boc(&shard_account.account_cell())?;
        std::fs::write(boc_name, &account_state)
            .map_err(|err| error!("Can`t create file: {}", err))?;

        Ok((format!("{} {}",
            hex::encode(&account_state),
            base64::encode(&account_state)),
            account_state)
        )
    }
}

impl SendReceive for SetStatesGcInterval {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let interval_ms_str = params.next().ok_or_else(|| error!("insufficient parameters"))?.to_string();
        let interval_ms = interval_ms_str.parse().map_err(|e| error!("can't parse <milliseconds>: {}", e))?;
        Ok(TLObject::new(ton::rpc::engine::validator::SetStatesGcInterval { interval_ms }))
    }
}

/// ControlClient
struct ControlClient{
    config: AdnlConsoleConfigJson,
    adnl: AdnlClient,
}

impl ControlClient {

    /// Connect to server
    async fn connect(mut config: AdnlConsoleConfigJson) -> Result<Self> {
        let client_config = config.config.take()
            .ok_or_else(|| error!("config must contain \"config\" section"))?;
        let (_, adnl_config) = AdnlClientConfig::from_json_config(client_config)?;
        Ok(Self {
            config,
            adnl: AdnlClient::connect(&adnl_config).await?,
        })
    }

    /// Shutdown client
    async fn shutdown(self) -> Result<()> {
        self.adnl.shutdown().await
    }

    async fn command(&mut self, cmd: &str) -> Result<(String, Vec<u8>)> {
        let result = shell_words::split(cmd)?;
        let mut params = result.iter();
        match params.next().expect("takes_value set for COMMANDS").as_str() {
            "recover_stake" => self.process_recover_stake(params).await,
            "ebid" |
            "election-bid" |
            "election_bid" => self.process_election_bid(params).await,
            "config_param" |
            "cparam" => self.process_config_param(params).await,
            name => self.process_command(name, params).await
        }
    }

    async fn process_command<Q: ToString>(
        &mut self,
        name: &str,
        params: impl Iterator<Item = Q> + Clone
    ) -> Result<(String, Vec<u8>)> {
        let query = command_send(name, params.clone())?;
        let boxed = ControlQuery {
            data: ton::bytes(serialize_boxed(&query)?)
        };
        #[cfg(feature = "telemetry")]
        let tag = tag_from_bare_object(&boxed);
        let boxed = TaggedTlObject {
            object: TLObject::new(boxed),
            #[cfg(feature = "telemetry")]
            tag
        };
        let answer = self.adnl.query(&boxed).await
            .map_err(|err| error!("Error receiving answer: {}", err))?;
        match answer.downcast::<ControlQueryError>() {
            Err(answer) => match command_receive(name, answer, params) {
                Err(answer) => fail!("Wrong response to {:?}: {:?}", query, answer),
                Ok(result) => Ok(result)
            }
            Ok(error) => fail!("Error response to {:?}: {:?}", query, error),
        }
    }

    async fn process_recover_stake<Q: ToString>(&mut self, mut params: impl Iterator<Item = Q>) -> Result<(String, Vec<u8>)> {
        let query_id = now() as u64;
        // recover-stake.fif
        let mut data = 0x47657424u32.to_be_bytes().to_vec();
        data.extend_from_slice(&query_id.to_be_bytes());
        let len = data.len() * 8;
        let body = BuilderData::with_raw(data, len)?;
        let body = body.into_cell()?;
        log::trace!("message body {}", body);
        let data = ton_types::write_boc(&body)?;
        let path = params.next().map(|path| path.to_string()).unwrap_or("recover-query.boc".to_string());
        std::fs::write(&path, &data)?;
        Ok((format!("Message body is {} saved to path {}", base64::encode(&data), path), data))
    }

    fn convert_to_uint(value: &[u8], bytes_count: usize) -> TokenValue {
        assert!(value.len() == bytes_count);
        TokenValue::Uint(Uint {
            number: BigUint::from_bytes_be(value),
            size: value.len() * 8,
        })
    }

    // @input elect_time expire_time <validator-query.boc>
    // @output validator-query.boc
    async fn process_election_bid<Q: ToString>(&mut self, mut params: impl Iterator<Item = Q>) -> Result<(String, Vec<u8>)> {
        let wallet_id = parse_any(self.config.wallet_id.as_ref(), "wallet_id", |value| {
            match value.strip_prefix("-1:") {
                Some(stripped) => Ok(hex::decode(stripped)?),
                None => fail!("use masterchain wallet")
            }
        })?;
        let elect_time = parse_int(params.next(), "elect_time")?;
        if elect_time <= 0 {
            fail!("<elect-utime> must be a positive integer")
        }
        let elect_time_str = elect_time.to_string();
        let expire_time = parse_int(params.next(), "expire_time")?;
        if expire_time <= elect_time {
            fail!("<expire-utime> must be a grater than elect_time")
        }
        let expire_time_str = expire_time.to_string();
        let max_factor = self.config.max_factor.ok_or_else(|| error!("you must give max_factor as real"))?;
        if max_factor < 1.0 || max_factor > 100.0 {
            fail!("<max-factor> must be a real number 1..100")
        }
        let max_factor = (max_factor * 65536.0) as u32;

        let (s, perm) = self.process_command("newkey", Vec::<String>::new().iter()).await?;
        log::trace!("{}", s);
        let perm_str = hex::encode_upper(&perm);

        let (s, pub_key) = self.process_command("exportpub", [&perm_str].iter()).await?;
        log::trace!("{}", s);

        let (s, _) = self.process_command("addpermkey", [&perm_str, &elect_time_str, &expire_time_str].iter()).await?;
        log::trace!("{}", s);

        let (s, _) = self.process_command("addtempkey", [&perm_str, &perm_str, &expire_time_str].iter()).await?;
        log::trace!("{}", s);

        let (s, adnl) = self.process_command("newkey", Vec::<String>::new().iter()).await?;
        log::trace!("{}", s);
        let adnl_str = hex::encode_upper(&adnl);

        let (s, _) = self.process_command("addadnl", [&adnl_str, "0"].iter()).await?;
        log::trace!("{}", s);

        let (s, _) = self.process_command("addvalidatoraddr", [&perm_str, &adnl_str, &elect_time_str].iter()).await?;
        log::trace!("{}", s);

        let (s, bls) = self.process_command("newkey", vec!["bls"].iter()).await?;
        log::trace!("{}", s);
        let bls_str = &hex::encode_upper(&bls)[..];

        let (s, bls_pub_key) = self.process_command("exportpub", vec![bls_str].iter()).await?;
        log::trace!("{}", s);

        let (s, _) = self.process_command("addblskey", vec![perm_str.clone(), bls_str.to_string(), elect_time_str].iter()).await?;
        log::trace!("{}", s);

        // validator-elect-req.fif
        let mut data = 0x654C5074u32.to_be_bytes().to_vec();
        data.extend_from_slice(&elect_time.to_be_bytes());
        data.extend_from_slice(&max_factor.to_be_bytes());
        data.extend_from_slice(&wallet_id);
        data.extend_from_slice(&adnl);
        let data_str = hex::encode_upper(&data);
        log::trace!("data to sign {}", data_str);
        let (s, signature) = self.process_command("sign", [&perm_str, &data_str].iter()).await?;
        log::trace!("{}", s);
        Ed25519KeyOption::from_public_key(&pub_key[..].try_into()?)
            .verify(&data, &signature)?;

        let contract = Contract::load(ELECTOR_ABI).expect("Elector's ABI must be valid");
        let process_new_stake_fn = contract
            .function(ELECTOR_PROCESS_NEW_STAKE_FUNC_NAME)
            .expect("Elector contract must have 'process_new_stake' function for elections")
            .clone();
        log::trace!("Use process new stake function '{}' with id={:08X}",
            ELECTOR_PROCESS_NEW_STAKE_FUNC_NAME, process_new_stake_fn.get_function_id());

        let time_now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let header: HashMap<_, _> = vec![("time".to_owned(), TokenValue::Time(time_now_ms))]
            .into_iter()
            .collect();        

        let query_id = now() as u64;

        let parameters = [
            Token::new("query_id", Self::convert_to_uint(&query_id.to_be_bytes(), 8)),
            Token::new("validator_pubkey", Self::convert_to_uint(&pub_key, 32)),
            Token::new("stake_at", Self::convert_to_uint(&elect_time.to_be_bytes(), 4)),
            Token::new("max_factor", Self::convert_to_uint(&max_factor.to_be_bytes(), 4)),
            Token::new("adnl_addr", Self::convert_to_uint(&adnl, 32)),
            Token::new("bls_key1", Self::convert_to_uint(&bls_pub_key[0..32], 32)), //256 bits
            Token::new("bls_key2", Self::convert_to_uint(&bls_pub_key[32..], 16)), //128 bits
            Token::new("signature", TokenValue::Bytes(signature.to_vec())),
        ];

        const INTERNAL_CALL: bool = true; //internal message

        let body = process_new_stake_fn
            .encode_input(&header, &parameters, INTERNAL_CALL, None, None)
            .and_then(|builder| builder.into_cell())?;

/*        // validator-elect-signed.fif
        let mut data = 0x4E73744Bu32.to_be_bytes().to_vec();
        data.extend_from_slice(&query_id.to_be_bytes());
        data.extend_from_slice(&pub_key);
        data.extend_from_slice(&elect_time.to_be_bytes());
        data.extend_from_slice(&max_factor.to_be_bytes());
        data.extend_from_slice(&adnl);

        data.extend_from_slice(&bls_pub_key[0..31]); // 256 bits

        let mut data2 = BuilderData::new();
        data2.append_raw(&bls_pub_key[32..], 18); // 128 bits
        let len = signature.len() * 8;
        data2.append_raw(signature.as_slice(), len)?;

        let len = data.len() * 8;
        let mut body = BuilderData::with_raw(data, len)?;
        let len = signature.len() * 8;
        body.append_reference(data2);

        /////////---------*/
        log::trace!("message body {}", body);
        let data = ton_types::write_boc(&body)?;
        let path = params.next().map(|path| path.to_string()).unwrap_or("validator-query.boc".to_string());
        std::fs::write(&path, &data)?;
        Ok((format!("Message body is {} saved to path {}", base64::encode(&data), path), data))
    }

    // @input index zerostate.json <config-param.boc>
    // @output config-param.boc
    async fn process_config_param<Q: ToString>(&mut self, mut params: impl Iterator<Item = Q>) -> Result<(String, Vec<u8>)> {
        let index = parse_int(params.next(), "index")?;
        if index < 0 {
            fail!("<index> must not be a negative integer")
        }
        let zerostate = parse_any(params.next(), "zerostate.json", |value| Ok(value.to_string()))?;
        let path = params.next().map(|path| path.to_string()).unwrap_or("config-param.boc".to_string());

        let zerostate = std::fs::read_to_string(&zerostate)
            .map_err(|err| error!("Can't read zerostate json file {} : {}", zerostate, err))?;
        let zerostate = 
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&zerostate)
                .map_err(|err| error!("Can't parse read zerostate json file: {}", err))?;
        let zerostate = ton_block_json::parse_state(&zerostate)
            .map_err(|err| error!("Can't parse read zerostate json file: {}", err))?;

        let key = SliceData::load_builder(index.write_to_new_cell()?)?;
        let config_param_cell = zerostate.read_custom()
            .map_err(|err| error!("Can't read McStateExtra from zerostate: {}", err))?
            .ok_or_else(|| error!("Can't find McStateExtra in zerostate"))?
            .config().config_params.get(key)
            .map_err(|err| error!("Can't read config param {} from zerostate: {}", index, err))?
            .ok_or_else(|| error!("Can't find config param {} in zerostate", index))?
            .reference_opt(0)
            .ok_or_else(|| error!("Can't parse config param {}: wrong format - no reference", index))?;

        let data = write_boc(&config_param_cell)
            .map_err(|err| error!("Can't serialize config param {}: {}", index, err))?;

        std::fs::write(&path, &data)
            .map_err(|err| error!("Can't write config param {} to file {}: {}", index, path, err))?;

        Ok((format!("Config param {} saved to path {}", index, path), data))
    }
}

#[derive(serde::Deserialize)]
struct AdnlConsoleConfigJson {
    config: Option<AdnlClientConfigJson>,
    wallet_id: Option<String>,
    max_factor: Option<f32>
}

#[tokio::main]
async fn main() {
    // init_test_log();
    let args = clap::App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(clap::Arg::with_name("CONFIG")
            .short("C")
            .long("config")
            .help("config for console")
            .required(true)
            .default_value("console.json")
            .takes_value(true)
            .number_of_values(1))
        .arg(clap::Arg::with_name("COMMANDS")
            .allow_hyphen_values(true)
            .short("c")
            .long("cmd")
            .help("schedule command")
            .multiple(true)
            .takes_value(true))
        .arg(clap::Arg::with_name("TIMEOUT")
            .short("t")
            .long("timeout")
            .help("timeout in batch mode")
            .takes_value(true)
            .number_of_values(1))
        .arg(clap::Arg::with_name("VERBOSE")
            .long("verbose")
            .help("verbose regim"))
        .arg(clap::Arg::with_name("JSON")
            .short("j")
            .long("json")
            .help("output in json format")
            .takes_value(false))
        .get_matches();

    if !args.is_present("JSON") {
        println!(
            "tonlabs console {}\nCOMMIT_ID: {}\nBUILD_DATE: {}\nCOMMIT_DATE: {}\nGIT_BRANCH: {}",
            env!("CARGO_PKG_VERSION"),
            env!("BUILD_GIT_COMMIT"),
            env!("BUILD_TIME") ,
            env!("BUILD_GIT_DATE"),
            env!("BUILD_GIT_BRANCH")
        );
    }

    if args.is_present("VERBOSE") {
        let encoder_boxed = Box::new(log4rs::encode::pattern::PatternEncoder::new("{m}{n}"));
        let console = log4rs::append::console::ConsoleAppender::builder()
            .encoder(encoder_boxed)
            .build();
        let config = log4rs::config::Config::builder()
            .appender(log4rs::config::Appender::builder().build("console", Box::new(console)))
            .build(log4rs::config::Root::builder().appender("console").build(log::LevelFilter::Trace))
            .unwrap();
        log4rs::init_config(config).unwrap();
    }

    let config = args.value_of("CONFIG").expect("required set for config");
    let config = std::fs::read_to_string(config).expect("Can't read config file");
    let config = serde_json::from_str(&config).expect("Can't parse config");
    let timeout = match args.value_of("TIMEOUT") {
        Some(timeout) => u64::from_str(timeout).expect("timeout must be set in microseconds"),
        None => 0
    };
    let timeout = Duration::from_micros(timeout);
    let mut client = ControlClient::connect(config).await.expect("Can't create client");
    if let Some(commands) = args.values_of("COMMANDS") {
        // batch mode - call commands and exit
        for command in commands {
            match client.command(command.trim_matches('\"')).await {
                Ok((result, _)) => println!("{}", result),
                Err(err) => println!("Error executing command: {}", err)
            }
            tokio::time::sleep(timeout).await;
        }
    } else {
        // interactive mode
        loop {
            let mut line = String::default();
            std::io::stdin().read_line(&mut line).expect("Can't read line from stdin");
            match line.trim_end() {
                "" => continue,
                "quit" => break,
                command => match client.command(command).await {
                    Ok((result, _)) => println!("{}", result),
                    Err(err) => println!("{}", err)
                }
            }
        }
    }
    client.shutdown().await.ok();
}

