use adnl::common::{KeyOption, serialize};
use adnl::client::{AdnlClient, AdnlClientConfig, AdnlClientConfigJson};
use clap::{Arg, App};
use ton_api::ton::{
    self, TLObject, 
    accountaddress::AccountAddress,
    engine::validator::ControlQueryError,
    rpc::engine::validator::ControlQuery,
};
use ton_block::{Serializable, BlockIdExt};
use ton_types::{error, fail, Result, BuilderData, serialize_toc};
use std::{
    convert::TryInto,
    env,
    str::FromStr,
    time::Duration,
};
use serde_json::{Map, Value};

include!("../common/src/test.rs");

trait SendReceive {
    fn send<Q: ToString>(params: impl Iterator<Item = Q>) -> Result<TLObject>;
    fn receive<Q: ToString>(
        answer: TLObject, 
        _params: impl Iterator<Item = Q>
    ) -> std::result::Result<(String, Vec<u8>), TLObject> {
        answer.downcast::<ton_api::ton::engine::validator::Success>()?;
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
        ) -> std::result::Result<(String, Vec<u8>), TLObject> {
            match name {
                $($name => $command::receive(answer, params), )*
                _ => Err(answer)
            }
        }
    };
}

commands! {
    NewKeypair, "newkey", "newkey\tgenerates new key pair on server"
    ExportPub, "exportpub", "exportpub <keyhash>\texports public key by key hash"
    Sign, "sign", "sign <keyhash> <data>\tsigns bytestring with privkey"
    AddValidatorPermKey, "addpermkey", "addpermkey <keyhash> <election-date> <expire-at>\tadd validator permanent key"
    AddValidatorTempKey, "addtempkey", "addtempkey <permkeyhash> <keyhash> <expire-at>\tadd validator temp key"
    AddValidatorAdnlAddr, "addvalidatoraddr", "addvalidatoraddr <permkeyhash> <keyhash> <expireat>\tadd validator ADNL addr"
    AddAdnlAddr, "addadnl", "addadnl <keyhash> <category>\tuse key as ADNL addr"
    Bundle, "bundle", "bundle <block_id>\tprepare bundle"
    FutureBundle, "future_bundle", "future_bundle <block_id>\tprepare future bundle"
    GetStats, "getstats", "getstats\tget status validator"
    GetSessionStats, "getconsensusstats", "getconsensusstats\tget consensus statistics for the node"
    SendMessage, "sendmessage", "sendmessage <filename>\tload a serialized message from <filename> and send it to server"
    GetAccountState, "getaccountstate", "getaccountstate <account id> <file name>\tsave accountstate to file"
}

fn parse_any<A, Q: ToString>(param_opt: Option<Q>, name: &str, parse_value: impl FnOnce(&str) -> Result<A>) -> Result<A> {
    param_opt
        .ok_or_else(|| error!("insufficient parameters"))
        .and_then(|value| parse_value(value.to_string().trim_matches('\"')))
        .map_err(|err| error!("you must give {}: {}", name, err))
}

fn parse_data<Q: ToString>(param_opt: Option<Q>, name: &str) -> Result<ton::bytes> {
    parse_any(
        param_opt,
        &format!("{} in hex format", name),
        |value| Ok(ton::bytes(hex::decode(value)?))
    )
}

fn parse_int256<Q: ToString>(param_opt: Option<Q>, name: &str) -> Result<ton::int256> {
    parse_any(
        param_opt,
        &format!("{} in hex or base64 format", name),
        |value| {
            let value = match value.len() {
                44 => base64::decode(value)?,
                64 => hex::decode(value)?,
                length => fail!("wrong hash: {} with length: {}", value, length)
            };
            Ok(ton::int256(value.as_slice().try_into()?))
        }
    )
}

fn parse_int<Q: ToString>(param_opt: Option<Q>, name: &str) -> Result<ton::int> {
    parse_any(param_opt, name, |value| Ok(ton::int::from_str(value)?))
}

fn parse_blockid<Q: ToString>(param_opt: Option<Q>, name: &str) -> Result<ton_api::ton::ton_node::blockidext::BlockIdExt> {
    parse_any(param_opt, name, |value| {
        let block_id = BlockIdExt::from_str(value)?;
        Ok(ton_node::block::convert_block_id_ext_blk2api(&block_id))
    })
}

fn now() -> ton::int {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as ton::int
}

impl SendReceive for GetStats {
    fn send<Q: ToString>(_params: impl Iterator<Item = Q>) -> Result<TLObject> {
        Ok(TLObject::new(ton::rpc::engine::validator::GetStats))
    }
    fn receive<Q: ToString>(
        answer: TLObject, 
        mut _params: impl Iterator<Item = Q>
    ) -> std::result::Result<(String, Vec<u8>), TLObject> {
        let data = serialize(&answer).unwrap();
        let stats = answer.downcast::<ton_api::ton::engine::validator::Stats>()?;
        let mut description = String::from("{");
        for stat in stats.stats().iter() {
            description.push_str("\n\t\"");
            description.push_str(&stat.key);
            description.push_str("\":\t");
            description.push_str(&stat.value);
            description.push_str(",");
        }
        description.pop();
        description.push_str("\n}");
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
    ) -> std::result::Result<(String, Vec<u8>), TLObject> {
        let data = serialize(&answer).unwrap();
        let stats = answer.downcast::<ton_api::ton::engine::validator::SessionStats>()?;
        let mut description = String::from("{");
        for session_stat in stats.stats().iter() {
            description.push_str("\n\t\"");
            description.push_str(&session_stat.session_id);
            description.push_str("\":\t{");
            for stat in session_stat.stats.iter() {
                description.push_str("\n\t\t\"");
                description.push_str(&stat.key);
                description.push_str("\":\t");
                description.push_str(&stat.value);
                description.push_str(",");
            }
            description.pop();
            description.push_str("\n\t\"},");
        }
        description.pop();
        description.push_str("\n}");
        Ok((description, data))
    }
}

impl SendReceive for NewKeypair {
    fn send<Q: ToString>(_params: impl Iterator<Item = Q>) -> Result<TLObject> {
        Ok(TLObject::new(ton::rpc::engine::validator::GenerateKeyPair))
    }
    fn receive<Q: ToString>(
        answer: TLObject, 
        mut _params: impl Iterator<Item = Q>
    ) -> std::result::Result<(String, Vec<u8>), TLObject> {
        let key_hash = answer.downcast::<ton_api::ton::engine::validator::KeyHash>()?.key_hash().0.to_vec();
        Ok((format!("received public key hash: {} {}", hex::encode(&key_hash), base64::encode(&key_hash)), key_hash))
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
    ) -> std::result::Result<(String, Vec<u8>), TLObject> {
        let pub_key = answer.downcast::<ton_api::ton::PublicKey>()?.key().unwrap().0.to_vec();
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
    ) -> std::result::Result<(String, Vec<u8>), TLObject> {
        let signature = answer.downcast::<ton_api::ton::engine::validator::Signature>()?.signature().0.clone();
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


impl SendReceive for GetAccountState {
    fn send<Q: ToString>(mut params: impl Iterator<Item = Q>) -> Result<TLObject> {
        let account = AccountAddress { 
            account_address: params.next().ok_or_else(|| error!("insufficient parameters"))?.to_string()
        };
        Ok(TLObject::new(ton::rpc::raw::GetAccountState {account_address: account }))
    }

    fn receive<Q: ToString>(
        answer: TLObject, 
        mut params: impl Iterator<Item = Q>
    ) -> std::result::Result<(String, Vec<u8>), TLObject> {
        let account_state = answer.downcast::<ton_api::ton::raw::FullAccountState>()?;

        /*let code_cell = deserialize_tree_of_cells(&mut Cursor::new(&account_state.code().0)).unwrap();
        let data_cell = deserialize_tree_of_cells(&mut Cursor::new(&account_state.data().0)).unwrap();

        let state_init = StateInit {
            split_depth: None,
            special: None,
            code: Some(code_cell),
            data: Some(data_cell),
            library: StateInitLib::default()
        };
        let state_init_raw = state_init.write_to_bytes().unwrap();
        */
        params.next();
        let boc_name = params.next().unwrap().to_string();
        std::fs::write(boc_name, account_state.data().0.clone())
            .map_err(|err| error!("Can`t create file: {}", err)).unwrap();

        Ok((format!("account state: {} {}",
            hex::encode(&account_state.data().0),
            base64::encode(&account_state.data().0)),
            account_state.data().0.clone())
        )
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
        match &params.next().expect("takes_value set for COMMANDS")[..] {
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
            data: ton::bytes(serialize(&query)?)
        };
        let answer = self.adnl.query(&TLObject::new(boxed)).await
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
        let body = body.into();
        log::trace!("message body {}", body);
        let data = ton_types::serialize_toc(&body)?;
        let path = params.next().map(|path| path.to_string()).unwrap_or("recover-query.boc".to_string());
        std::fs::write(&path, &data)?;
        Ok((format!("Message body is {} saved to path {}", base64::encode(&data), path), data))
    }

    // @input elect_time expire_time <validator-query.boc>
    // @output validator-query.boc
    async fn process_election_bid<Q: ToString>(&mut self, mut params: impl Iterator<Item = Q>) -> Result<(String, Vec<u8>)> {
        let wallet_id = parse_any(self.config.wallet_id.as_ref(), "wallet_id", |value| {
            if !value.starts_with("-1:") {
                fail!("use masterchain wallet")
            }
            Ok(hex::decode(&value[3..])?)
        })?;
        let elect_time = parse_int(params.next(), "elect_time")?;
        if elect_time <= 0 {
            fail!("<elect-utime> must be a positive integer")
        }
        let elect_time_str = &format!("{}", elect_time)[..];
        let expire_time = parse_int(params.next(), "expire_time")?;
        if expire_time <= elect_time {
            fail!("<expire-utime> must be a grater than elect_time")
        }
        let expire_time_str = &format!("{}", expire_time)[..];
        let max_factor = self.config.max_factor.ok_or_else(|| error!("you must give max_factor as real"))?;
        if max_factor < 1.0 || max_factor > 100.0 {
            fail!("<max-factor> must be a real number 1..100")
        }
        let max_factor = (max_factor * 65536.0) as u32;

        let (s, perm) = self.process_command("newkey", Vec::<String>::new().iter()).await?;
        log::trace!("{}", s);
        let perm_str = &hex::encode_upper(&perm)[..];

        let (s, pub_key) = self.process_command("exportpub", vec![perm_str].iter()).await?;
        log::trace!("{}", s);

        let (s, _) = self.process_command("addpermkey", vec![perm_str, elect_time_str, expire_time_str].iter()).await?;
        log::trace!("{}", s);

        let (s, _) = self.process_command("addtempkey", vec![perm_str, perm_str, expire_time_str].iter()).await?;
        log::trace!("{}", s);

        let (s, adnl) = self.process_command("newkey", Vec::<String>::new().iter()).await?;
        log::trace!("{}", s);
        let adnl_str = &hex::encode_upper(&adnl)[..];

        let (s, _) = self.process_command("addadnl", vec![adnl_str, "0"].iter()).await?;
        log::trace!("{}", s);

        let (s, _) = self.process_command("addvalidatoraddr", vec![perm_str, adnl_str, elect_time_str].iter()).await?;
        log::trace!("{}", s);

        // validator-elect-req.fif
        let mut data = 0x654C5074u32.to_be_bytes().to_vec();
        data.extend_from_slice(&elect_time.to_be_bytes());
        data.extend_from_slice(&max_factor.to_be_bytes());
        data.extend_from_slice(&wallet_id);
        data.extend_from_slice(&adnl);
        log::trace!("data to sign {}", hex::encode_upper(&data));
        let data_str = &hex::encode_upper(&data)[..];
        let (s, signature) = self.process_command("sign", vec![perm_str, data_str].iter()).await?;
        log::trace!("{}", s);
        KeyOption::from_type_and_public_key(KeyOption::KEY_ED25519, &pub_key[..].try_into()?)
            .verify(&data, &signature)?;

        let query_id = now() as u64;
        // validator-elect-signed.fif
        let mut data = 0x4E73744Bu32.to_be_bytes().to_vec();
        data.extend_from_slice(&query_id.to_be_bytes());
        data.extend_from_slice(&pub_key);
        data.extend_from_slice(&elect_time.to_be_bytes());
        data.extend_from_slice(&max_factor.to_be_bytes());
        data.extend_from_slice(&adnl);
        let len = data.len() * 8;
        let mut body = BuilderData::with_raw(data, len)?;
        let len = signature.len() * 8;
        body.append_reference(BuilderData::with_raw(signature, len)?);
        let body = body.into();
        log::trace!("message body {}", body);
        let data = ton_types::serialize_toc(&body)?;
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
        let zerostate = serde_json::from_str::<Map<String, Value>>(&zerostate)
            .map_err(|err| error!("Can't parse read zerostate json file: {}", err))?;
        let zerostate = ton_block_json::parse_state(&zerostate)
            .map_err(|err| error!("Can't parse read zerostate json file: {}", err))?;

        let config_param_cell = zerostate.read_custom()
            .map_err(|err| error!("Can't read McStateExtra from zerostate: {}", err))?
            .ok_or_else(|| error!("Can't find McStateExtra in zerostate"))?
            .config().config_params.get(index.serialize()?.into())
            .map_err(|err| error!("Can't read config param {} from zerostate: {}", index, err))?
            .ok_or_else(|| error!("Can't find config param {} in zerostate", index))?
            .reference_opt(0)
            .ok_or_else(|| error!("Can't parse config param {}: wrong format - no reference", index))?;

        let data = serialize_toc(&config_param_cell)
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
    println!(
        "tonlabs console {}\nCOMMIT_ID: {}\nBUILD_DATE: {}\nCOMMIT_DATE: {}\nGIT_BRANCH: {}",
        env!("CARGO_PKG_VERSION"),
        env!("BUILD_GIT_COMMIT"),
        env!("BUILD_TIME") ,
        env!("BUILD_GIT_DATE"),
        env!("BUILD_GIT_BRANCH")
    );
    // init_test_log();
    let args = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(Arg::with_name("CONFIG")
            .short("C")
            .long("config")
            .help("config for console")
            .required(true)
            .default_value("console.json")
            .takes_value(true)
            .number_of_values(1))
        .arg(Arg::with_name("COMMANDS")
            .allow_hyphen_values(true)
            .short("c")
            .long("cmd")
            .help("schedule command")
            .multiple(true)
            .takes_value(true))
        .arg(Arg::with_name("TIMEOUT")
            .short("t")
            .long("timeout")
            .help("timeout in batch mode")
            .takes_value(true)
            .number_of_values(1))
        .get_matches();

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

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::Arc;
    use ton_node::{
        config::{NodeConfigHandler, TonNodeConfig},
        network::control::ControlServer,
    };
    use ton_types::Result;
    use ton_block::{BlockLimits, ParamLimits, Deserializable};

    pub struct Engine {
        _config_handler: Arc<NodeConfigHandler>,
        control: ControlServer
    }

    impl Engine {
        pub async fn with_config(node_config_path: &str) -> Result<Self> {
            let node_config = TonNodeConfig::from_file("target", node_config_path, None, "", None)?;
            let control_server_config = node_config.control_server()?;
            let (config_handler, context) = NodeConfigHandler::create(node_config, &tokio::runtime::Handle::current())?;
            NodeConfigHandler::start_sheduler(config_handler.clone(), context, vec![])?;
            let config = control_server_config.expect("must have control server setting");
            let control = ControlServer::with_config(
                config, None, config_handler.clone(), config_handler.clone()
            ).await?;

            Ok(Self {
                _config_handler: config_handler.clone(),
                control
            })
        }
        pub async fn shutdown(self) {
            self.control.shutdown().await
        }
    }
    const ADNL_SERVER_CONFIG: &str = r#"{
        "control_server": {
            "address": "127.0.0.1:4924",
            "server_key": {
                "type_id": 1209251014,
                "pvt_key": "cJIxGZviebMQWL726DRejqVzRTSXPv/1sO/ab6XOZXk="
            },
            "clients": {
                "list": [
                    {
                        "type_id": 1209251014,
                        "pub_key": "RYokIiD5AFkzfTBgC6NhtAGFKm0+gwhN4suTzaW0Sjw="
                    }
                ]
            }
        }
    }"#;

    const ADNL_CLIENT_CONFIG: &str = r#"{
        "config": {
            "server_address": "127.0.0.1:4924",
            "server_key": {
                "type_id": 1209251014,
                "pub_key": "cujCRU4rQbSw48yHVHxQtRPhUlbo+BuZggFTQSu04Y8="
            },
            "client_key": {
                "type_id": 1209251014,
                "pvt_key": "oEivbTDjSOSCgooUM0DAS2z2hIdnLw/PT82A/OFLDmA="
            }
        },
        "wallet_id": "-1:af17db43f40b6aa24e7203a9f8c8652310c88c125062d1129fe883eaa1bd6763",
        "max_factor": 2.7
    }"#;

    const SAMPLE_ZERO_STATE: &str = r#"{
        "id": "-1:8000000000000000",
        "workchain_id": -1,
        "boc": "",
        "global_id": 42,
        "shard": "8000000000000000",
        "seq_no": 0,
        "vert_seq_no": 0,
        "gen_utime": 1600000000,
        "gen_lt": "0",
        "min_ref_mc_seqno": 4294967295,
        "before_split": false,
        "overload_history": "0",
        "underload_history": "0",
        "total_balance": "4993357197000000000",
        "total_validator_fees": "0",
        "master": {
            "config_addr": "5555555555555555555555555555555555555555555555555555555555555555",
            "config": {
            "p0": "5555555555555555555555555555555555555555555555555555555555555555",
            "p1": "3333333333333333333333333333333333333333333333333333333333333333",
            "p2": "0000000000000000000000000000000000000000000000000000000000000000",
            "p7": [],
            "p8": {
                "version": 5,
                "capabilities": "46"
            },
            "p9": [ 0 ],
            "p10": [ 0 ],
            "p11": {
                "normal_params": {
                    "min_tot_rounds": 2,
                    "max_tot_rounds": 3,
                    "min_wins": 2,
                    "max_losses": 2,
                    "min_store_sec": 1000000,
                    "max_store_sec": 10000000,
                    "bit_price": 1,
                    "cell_price": 500
                },
                "critical_params": {
                    "min_tot_rounds": 4,
                    "max_tot_rounds": 7,
                    "min_wins": 4,
                    "max_losses": 2,
                    "min_store_sec": 5000000,
                    "max_store_sec": 20000000,
                    "bit_price": 2,
                    "cell_price": 1000
                }
            },
            "p12": [],
            "p13": {
                "boc": "te6ccgEBAQEADQAAFRpRdIdugAEBIB9I"
            },
            "p14": {
                "masterchain_block_fee": "1700000000",
                "basechain_block_fee": "1000000000"
            },
            "p15": {
                "validators_elected_for": 65536,
                "elections_start_before": 32768,
                "elections_end_before": 8192,
                "stake_held_for": 32768
            },
            "p16": {
                "max_validators": 1000,
                "max_main_validators": 100,
                "min_validators": 5
            },
            "p17": {
                "min_stake": "10000000000000",
                "max_stake": "10000000000000000",
                "min_total_stake": "100000000000000",
                "max_stake_factor": 196608
            },
            "p18": [
                {
                "utime_since": 0,
                "bit_price_ps": "1",
                "cell_price_ps": "500",
                "mc_bit_price_ps": "1000",
                "mc_cell_price_ps": "500000"
                }
            ],
            "p20": {
                "flat_gas_limit": "1000",
                "flat_gas_price": "10000000",
                "gas_price": "655360000",
                "gas_limit": "1000000",
                "special_gas_limit": "100000000",
                "gas_credit": "10000",
                "block_gas_limit": "10000000",
                "freeze_due_limit": "100000000",
                "delete_due_limit": "1000000000"
            },
            "p21": {
                "flat_gas_limit": "1000",
                "flat_gas_price": "1000000",
                "gas_price": "65536000",
                "gas_limit": "1000000",
                "special_gas_limit": "1000000",
                "gas_credit": "10000",
                "block_gas_limit": "10000000",
                "freeze_due_limit": "100000000",
                "delete_due_limit": "1000000000"
            },
            "p22": {
                "bytes": {
                    "underload": 131072,
                    "soft_limit": 524288,
                    "hard_limit": 1048576
                },
                "gas": {
                    "underload": 900000,
                    "soft_limit": 1200000,
                    "hard_limit": 2000000
                },
                "lt_delta": {
                    "underload": 1000,
                    "soft_limit": 5000,
                    "hard_limit": 10000
                }
            },
            "p23": {
                "bytes": {
                    "underload": 131072,
                    "soft_limit": 524288,
                    "hard_limit": 1048576
                },
                "gas": {
                    "underload": 900000,
                    "soft_limit": 1200000,
                    "hard_limit": 2000000
                },
                "lt_delta": {
                    "underload": 1000,
                    "soft_limit": 5000,
                    "hard_limit": 10000
                }
            },
            "p24": {
                "lump_price": "10000000",
                "bit_price": "655360000",
                "cell_price": "65536000000",
                "ihr_price_factor": 98304,
                "first_frac": 21845,
                "next_frac": 21845
            },
            "p25": {
                "lump_price": "1000000",
                "bit_price": "65536000",
                "cell_price": "6553600000",
                "ihr_price_factor": 98304,
                "first_frac": 21845,
                "next_frac": 21845
            },
            "p28": {
                "shuffle_mc_validators": true,
                "mc_catchain_lifetime": 250,
                "shard_catchain_lifetime": 250,
                "shard_validators_lifetime": 1000,
                "shard_validators_num": 7
            },
            "p29": {
                "new_catchain_ids": true,
                "round_candidates": 3,
                "next_candidate_delay_ms": 2000,
                "consensus_timeout_ms": 16000,
                "fast_attempts": 3,
                "attempt_duration": 8,
                "catchain_max_deps": 4,
                "max_block_bytes": 2097152,
                "max_collated_bytes": 2097152
            },
            "p31": [
                "0000000000000000000000000000000000000000000000000000000000000000"
            ],
            "p34": {
                "utime_since": 1600000000,
                "utime_until": 1610000000,
                "total": 1,
                "main": 1,
                "total_weight": "17",
                "list": [
                    {
                        "public_key": "2e7eb5a711ed946605a91e36037c4cb927181eff4bb277b175d891a588d03536",
                        "weight": "17"
                    }
                ]
            }
            },
            "validator_list_hash_short": 871956759,
            "catchain_seqno": 0,
            "nx_cc_updated": true,
            "after_key_block": true,
            "global_balance": "4993357197000000000"
        },
        "accounts": [],
        "libraries": [],
        "out_msg_queue_info": {
            "out_queue": [],
            "proc_info": [],
            "ihr_pending": []
        }
    }"#;

    async fn test_one_cmd(cmd: &str, check_result: impl FnOnce(Vec<u8>)) {
        // init_test_log();
        std::fs::write("target/light_node.json", ADNL_SERVER_CONFIG).unwrap();
        let server = Engine::with_config("light_node.json").await.unwrap();
        let config = serde_json::from_str(&ADNL_CLIENT_CONFIG).unwrap();
        let mut client = ControlClient::connect(config).await.unwrap();
        let (_, result) = client.command(cmd).await.unwrap();
        check_result(result);
        client.shutdown().await.ok();
        server.shutdown().await;
    }

    #[tokio::test]
    async fn test_new_key_one() {
        let cmd = "newkey";
        test_one_cmd(cmd, |result| assert_eq!(result.len(), 32)).await;
    }

    #[tokio::test]
    async fn test_validator_status() {
        let cmd = "get_validator_status";
        test_one_cmd(cmd, |result| assert_eq!(result, vec![0,0])).await;
    }

    #[tokio::test]
    async fn test_election_bid() {
        let now = now() + 86400;
        let cmd = format!(r#"election-bid {} {} "target/validator-query.boc""#, now, now + 10001);
        test_one_cmd(&cmd, |result| assert_eq!(result.len(), 164)).await;
    }

    #[tokio::test]
    async fn test_recover_stake() {
        let cmd = r#"recover_stake "target/recover-query.boc""#;
        test_one_cmd(cmd, |result| assert_eq!(result.len(), 25)).await;
    }

    #[tokio::test]
    async fn test_config_param() {
        std::fs::write("target/zerostate.json", SAMPLE_ZERO_STATE).unwrap();
        let cmd = r#"cparam 23 "target/zerostate.json" "target/config-param.boc""#;
        test_one_cmd(cmd, |result| {
            assert_eq!(result.len(), 53);
            let limits = BlockLimits::construct_from_bytes(&result).unwrap();
            assert_eq!(limits.bytes(), &ParamLimits::with_limits(131072, 524288, 1048576).unwrap());
            assert_eq!(limits.gas(), &ParamLimits::with_limits(900000, 1200000, 2000000).unwrap());
            assert_eq!(limits.lt_delta(), &ParamLimits::with_limits(1000, 5000, 10000).unwrap());
        }).await;
    }

    #[tokio::test]
    async fn test_new_key_with_export() {
        // init_test_log();
        std::fs::write("target/light_node.json", ADNL_SERVER_CONFIG).unwrap();
        let server = Engine::with_config("light_node.json").await.unwrap();
        let config = serde_json::from_str(&ADNL_CLIENT_CONFIG).unwrap();
        let mut client = ControlClient::connect(config).await.unwrap();
        let (_, result) = client.command("newkey").await.unwrap();
        assert_eq!(result.len(), 32);

        let cmd = format!("exportpub {}", base64::encode(&result));
        let (_, result) = client.command(&cmd).await.unwrap();
        assert_eq!(result.len(), 32);

        client.shutdown().await.ok();
        server.shutdown().await;
    }

    macro_rules! parse_test {
        ($func:expr, $param:expr) => {
            $func($param.split_whitespace().next(), "test")
        };
    }
    #[test]
    fn test_parse_int() {
        assert_eq!(parse_test!(parse_int, "0").unwrap(), 0);
        assert_eq!(parse_test!(parse_int, "-1").unwrap(), -1);
        assert_eq!(parse_test!(parse_int, "1600000000").unwrap(), 1600000000);

        parse_test!(parse_int, "qwe").expect_err("must generate error");
        parse_int(Option::<&str>::None, "test").expect_err("must generate error");
    }

    #[test]
    fn test_parse_int256() {
        let ethalon = ton::int256(base64::decode("GfgI79Xf3q7r4q1SPz7wAqBt0W6CjavuADODoz/DQE8=").unwrap().as_slice().try_into().unwrap());
        assert_eq!(parse_test!(parse_int256, "GfgI79Xf3q7r4q1SPz7wAqBt0W6CjavuADODoz/DQE8=").unwrap(), ethalon);
        assert_eq!(parse_test!(parse_int256, "19F808EFD5DFDEAEEBE2AD523F3EF002A06DD16E828DABEE003383A33FC3404F").unwrap(), ethalon);

        parse_test!(parse_int256, "11").expect_err("must generate error");
        parse_int256(Option::<&str>::None, "test").expect_err("must generate error");
    }

    #[test]
    fn test_parse_data() {
        let ethalon = ton::bytes(vec![10, 77]);
        assert_eq!(parse_test!(parse_data, "0A4D").unwrap(), ethalon);

        parse_test!(parse_data, "QQ").expect_err("must generate error");
        parse_test!(parse_data, "GfgI79Xf3q7r4q1SPz7wAqBt0W6CjavuADODoz/DQE8=").expect_err("must generate error");
        parse_data(Option::<&str>::None, "test").expect_err("must generate error");
    }
}
