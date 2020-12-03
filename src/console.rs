use adnl::common::{KeyOption, serialize};
use adnl::client::{AdnlClient, AdnlClientConfig, AdnlClientConfigJson};
use ton_api::{
    ton::{
        self, TLObject, 
        engine::validator::ControlQueryError,
        rpc::engine::validator::ControlQuery,
    }
};
use ton_types::{error, fail, Result, BuilderData};
use clap::{Arg, App};
use std::convert::TryInto;
use std::env;
use std::str::FromStr;

include!("../../common/src/log.rs");

trait SendReceive {
    fn send<'a>(params: impl Iterator<Item = &'a str>) -> Result<TLObject>;
    fn receive(answer: TLObject) -> std::result::Result<(String, Vec<u8>), TLObject> {
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
        fn command_send<'a>(name: &str, params: impl Iterator<Item = &'a str>) -> Result<TLObject> {
            match name {
                $($name => $command::send(params), )*
                _ => fail!("command {} not supported", name)
            }
        }
        fn command_receive(name: &str, answer: TLObject) -> std::result::Result<(String, Vec<u8>), TLObject> {
            match name {
                $($name => $command::receive(answer), )*
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
}

fn parse_any<A, T: AsRef<str>>(param_opt: Option<T>, name: &str, parse_value: impl FnOnce(&str) -> Result<A>) -> Result<A> {
    param_opt
        .ok_or_else(|| error!("insufficient parameters"))
        .and_then(|value| parse_value(value.as_ref()))
        .map_err(|_| error!("you must give {}", name))
}

fn parse_data<T: AsRef<str>>(param_opt: Option<T>, name: &str) -> Result<ton::bytes> {
    parse_any(
        param_opt,
        &format!("{} in hex format", name),
        |value| Ok(ton::bytes(hex::decode(value)?))
    )
}

fn parse_int256<T: AsRef<str>>(param_opt: Option<T>, name: &str) -> Result<ton::int256> {
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

fn parse_int<T: AsRef<str>>(param_opt: Option<T>, name: &str) -> Result<ton::int> {
    parse_any(param_opt, name, |value| Ok(ton::int::from_str(value)?))
}

#[cfg(test)]
fn parse_test<A>(parse: impl FnOnce(Option<&str>, &str) -> Result<A>, param: &str) -> Result<A> {
    parse(param.split_whitespace().next(), "test")
}

#[test]
fn test_parse_int() {
    assert_eq!(parse_test(parse_int, "0").unwrap(), 0);
    assert_eq!(parse_test(parse_int, "-1").unwrap(), -1);
    assert_eq!(parse_test(parse_int, "1600000000").unwrap(), 1600000000);

    parse_test(parse_int, "qwe").expect_err("must generate error");
    parse_int(None, "test").expect_err("must generate error");
}

#[test]
fn test_parse_int256() {
    let ethalon = ton::int256(base64::decode("GfgI79Xf3q7r4q1SPz7wAqBt0W6CjavuADODoz/DQE8=").unwrap().as_slice().try_into().unwrap());
    assert_eq!(parse_test(parse_int256, "GfgI79Xf3q7r4q1SPz7wAqBt0W6CjavuADODoz/DQE8=").unwrap(), ethalon);
    assert_eq!(parse_test(parse_int256, "19F808EFD5DFDEAEEBE2AD523F3EF002A06DD16E828DABEE003383A33FC3404F").unwrap(), ethalon);

    parse_test(parse_int256, "11").expect_err("must generate error");
    parse_int256(None, "test").expect_err("must generate error");
}

#[test]
fn test_parse_data() {
    let ethalon = ton::bytes(vec![10, 77]);
    assert_eq!(parse_test(parse_data, "0A4D").unwrap(), ethalon);

    parse_test(parse_data, "QQ").expect_err("must generate error");
    parse_test(parse_data, "GfgI79Xf3q7r4q1SPz7wAqBt0W6CjavuADODoz/DQE8=").expect_err("must generate error");
    parse_data(None, "test").expect_err("must generate error");
}

impl SendReceive for NewKeypair {
    fn send<'a>(_params: impl Iterator<Item = &'a str>) -> Result<TLObject> {
        Ok(TLObject::new(ton::rpc::engine::validator::GenerateKeyPair))
    }
    fn receive(answer: TLObject) -> std::result::Result<(String, Vec<u8>), TLObject> {
        let key_hash = answer.downcast::<ton_api::ton::engine::validator::KeyHash>()?.key_hash().0.to_vec();
        Ok((format!("received public key hash: {} {}", hex::encode(&key_hash), base64::encode(&key_hash)), key_hash))
    }
}

impl SendReceive for ExportPub {
    fn send<'a>(mut params: impl Iterator<Item = &'a str>) -> Result<TLObject> {
        let key_hash = parse_int256(params.next(), "key_hash")?;
        Ok(TLObject::new(ton::rpc::engine::validator::ExportPublicKey {
            key_hash
        }))
    }
    fn receive(answer: TLObject) -> std::result::Result<(String, Vec<u8>), TLObject> {
        let pub_key = answer.downcast::<ton_api::ton::PublicKey>()?.key().unwrap().0.to_vec();
        Ok((format!("imported key: {} {}", hex::encode(&pub_key), base64::encode(&pub_key)), pub_key))
    }
}

impl SendReceive for Sign {
    fn send<'a>(mut params: impl Iterator<Item = &'a str>) -> Result<TLObject> {
        let key_hash = parse_int256(params.next(), "key_hash")?;
        let data = parse_data(params.next(), "data")?;
        Ok(TLObject::new(ton::rpc::engine::validator::Sign {
            key_hash,
            data
        }))
    }
    fn receive(answer: TLObject) -> std::result::Result<(String, Vec<u8>), TLObject> {
        let signature = answer.downcast::<ton_api::ton::engine::validator::Signature>()?.signature().0.clone();
        Ok((format!("got signature: {} {}", hex::encode(&signature), base64::encode(&signature)), signature))
    }
}

impl SendReceive for AddValidatorPermKey {
    fn send<'a>(mut params: impl Iterator<Item = &'a str>) -> Result<TLObject> {
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
    fn send<'a>(mut params: impl Iterator<Item = &'a str>) -> Result<TLObject> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as ton::int;
        let permanent_key_hash = parse_int256(params.next(), "permanent_key_hash")?;
        let key_hash = parse_int256(params.next(), "key_hash")?;
        let ttl = parse_int(params.next(), "expire_at")? - now;
        Ok(TLObject::new(ton::rpc::engine::validator::AddValidatorTempKey {
            permanent_key_hash,
            key_hash,
            ttl
        }))
    }
}

impl SendReceive for AddValidatorAdnlAddr {
    fn send<'a>(mut params: impl Iterator<Item = &'a str>) -> Result<TLObject> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as ton::int;
        let permanent_key_hash = parse_int256(params.next(), "permanent_key_hash")?;
        let key_hash = parse_int256(params.next(), "key_hash")?;
        let ttl = parse_int(params.next(), "expire_at")? - now;
        Ok(TLObject::new(ton::rpc::engine::validator::AddValidatorAdnlAddress {
            permanent_key_hash,
            key_hash,
            ttl
        }))
    }
}

impl SendReceive for AddAdnlAddr {
    fn send<'a>(mut params: impl Iterator<Item = &'a str>) -> Result<TLObject> {
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

/// ControlClient
struct ControlClient{
    config: AdnlConsoleConfigJson,
    adnl: AdnlClient,
}

impl ControlClient {

    /// Connect to server
    async fn connect(mut config: AdnlConsoleConfigJson) -> Result<Self> {
        let adnl_config = AdnlClientConfig::from_json_config(config.config.take().unwrap())?;
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
        let mut params = cmd.split_whitespace();
        match params.next().expect("takes_value set for COMMANDS") {
            "test" => self.process_test().await,
            "election-bid" => self.process_election_bid(params).await,
            name => self.process_command(name, params).await
        }
    }

    async fn process_command<'a>(&mut self, name: &str, params: impl Iterator<Item = &'a str>) -> Result<(String, Vec<u8>)> {
        let query = command_send(name, params)?;
        let boxed = ControlQuery {
            data: ton::bytes(serialize(&query)?)
        };
        let answer = self.adnl.query(&TLObject::new(boxed)).await
            .map_err(|err| error!("Error receiving answer: {}", err))?;
        match answer.downcast::<ControlQueryError>() {
            Err(answer) => match command_receive(name, answer) {
                Err(answer) => fail!("Wrong response to {:?}: {:?}", query, answer),
                Ok(result) => Ok(result)
            }
            Ok(error) => fail!("Error response to {:?}: {:?}", query, error),
        }
    }

    async fn process_election_bid<'a>(&mut self, mut params: impl Iterator<Item = &'a str>) -> Result<(String, Vec<u8>)> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as ton::int;
        let wallet_id = parse_int256(self.config.wallet_id.as_ref(), "wallet_id")?;
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

        let (s, perm) = self.process_command("newkey", vec![].drain(..)).await?;
        log::trace!("{}", s);
        let perm_str = &hex::encode_upper(&perm)[..];

        let (s, pub_key) = self.process_command("exportpub", vec![perm_str].drain(..)).await?;
        log::trace!("{}", s);

        let (s, _) = self.process_command("addpermkey", vec![perm_str, elect_time_str, expire_time_str].drain(..)).await?;
        log::trace!("{}", s);

        let (s, _) = self.process_command("addtempkey", vec![perm_str, perm_str, expire_time_str].drain(..)).await?;
        log::trace!("{}", s);

        let (s, adnl) = self.process_command("newkey", vec![].drain(..)).await?;
        log::trace!("{}", s);
        let adnl_str = &hex::encode_upper(&adnl)[..];

        let (s, _) = self.process_command("addadnl", vec![adnl_str, "0"].drain(..)).await?;
        log::trace!("{}", s);

        let (s, _) = self.process_command("addvalidatoraddr", vec![perm_str, adnl_str, elect_time_str].drain(..)).await?;
        log::trace!("{}", s);

        // validator-elect-req.fif
        let mut data = 0x654C5074u32.to_be_bytes().to_vec();
        data.extend_from_slice(&elect_time.to_be_bytes());
        data.extend_from_slice(&max_factor.to_be_bytes());
        data.extend_from_slice(&wallet_id);
        data.extend_from_slice(&adnl);
        log::trace!("data to sign {}", hex::encode_upper(&data));
        let data_str = &hex::encode_upper(&data)[..];
        let (s, signature) = self.process_command("sign", vec![perm_str, data_str].drain(..)).await?;
        log::trace!("{}", s);
        KeyOption::from_type_and_public_key(KeyOption::KEY_ED25519, &pub_key[..].try_into()?)
            .verify(&data, &signature)?;

        // validator-elect-signed.fif
        let mut data = 0x4E73744Bu32.to_be_bytes().to_vec();
        data.extend_from_slice(&now.to_be_bytes());
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
        let path = params.next().unwrap_or("validator-query.boc");
        std::fs::write(path, &data)?;
        Ok((format!("Message body is {}", path), data))
    }

    async fn process_test(&mut self) -> Result<(String, Vec<u8>)> {
        let (s, adnl) = self.process_command("newkey", vec![].drain(..)).await?;
        log::trace!("{}", s);
        let key_hash = &hex::encode_upper(&adnl)[..];
        let wallet_id = "kf-vF9tD9Atqok5yA6n4yGUjEMiMElBi0RKf6IPqob1nY2dP";
        let election_date = "1567633899";
        let max_factor = "2.7";
        let (s, body) = self.process_election_bid(vec![wallet_id, election_date, max_factor, key_hash].drain(..)).await?;
        log::trace!("{}", s);
        Ok((format!("test result"), body))
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
    init_test_log();
    let args = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(Arg::with_name("CONFIG")
            .short("C")
            .long("config")
            .help("config for console")
            .required(true)
            .takes_value(true)
            .number_of_values(1))
        // .arg(Arg::with_name("ELECTION-BID")
        //     .short("b")
        //     .long("election-bid")
        //     .help("prepare election bid")
        //     .takes_value(true)
        //     .number_of_values(2))
        .arg(Arg::with_name("COMMANDS")
            .short("c")
            .long("cmd")
            .help("schedule command")
            .multiple(true)
            .takes_value(true)
            .number_of_values(1))
        .arg(Arg::with_name("TIMEOUT")
            .short("t")
            .long("timeout")
            .help("timeout in batch mode")
            .takes_value(true)
            .number_of_values(1))
        .get_matches();

    let config = args.value_of("CONFIG").expect("required set for config");
    let config = serde_json::from_str(&std::fs::read_to_string(config).unwrap()).unwrap();
    let timeout = match args.value_of("TIMEOUT") {
        Some(timeout) => u64::from_str(timeout).expect("timeout must be set in microseconds"),
        None => 0
    };
    let timeout = std::time::Duration::from_micros(timeout);
    let mut client = ControlClient::connect(config).await.unwrap();
    if let Some(commands) = args.values_of("COMMANDS") {
        // batch mode - call commands and exit
        for command in commands {
            match client.command(command.trim_matches('\"')).await {
                Ok((result, _)) => println!("{}", result),
                Err(err) => println!("Error executing comamnd: {}", err)
            }
            tokio::time::delay_for(timeout).await;
        }
    } else {
        // interactive mode
        loop {
            let mut line = String::default();
            std::io::stdin().read_line(&mut line).unwrap();
            match line.trim_end() {
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
