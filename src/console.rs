use adnl::common::serialize;
use adnl::client::{AdnlClient, AdnlClientConfig};
use ton_api::{
    ton::{
        self, TLObject, 
        engine::validator::ControlQueryError,
        rpc::engine::validator::ControlQuery,
    }
};
use ton_types::{error, fail, Result};
use clap::{Arg, App};
use std::convert::TryInto;
use std::env;
use std::str::FromStr;

include!("../../common/src/log.rs");

trait SendReceive {
    fn send<'a>(params: impl Iterator<Item = &'a str>) -> Result<TLObject>;
    fn receive(answer: TLObject) -> std::result::Result<(), TLObject> {
        answer.downcast::<ton_api::ton::engine::validator::Success>()
            .map(|_| println!("success"))
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
        fn command_receive(name: &str, answer: TLObject) -> std::result::Result<(), TLObject> {
            match name {
                $($name => $command::receive(answer), )*
                _ => Err(answer)
            }
        }
    };
}

commands! {
    NewKeypair, "newkey", "newkey\tgenerates new key pair on server"
    Sign, "sign", "sign <keyhash> <data>\tsigns bytestring with privkey"
    AddValidatorPermKey, "addpermkey", "addpermkey <keyhash> <election-date> <expire-at>\tadd validator permanent key"
    AddValidatorTempKey, "addtempkey", "addtempkey <permkeyhash> <keyhash> <expire-at>\tadd validator temp key"
    AddValidatorAdnlAddr, "addvalidatoraddr", "addvalidatoraddr <permkeyhash> <keyhash> <expireat>\tadd validator ADNL addr"
    AddAdnlAddr, "addadnl", "addadnl <keyhash> <category>\tuse key as ADNL addr"
}

fn parse_any<A>(param_opt: Option<&str>, name: &str, parse_value: impl FnOnce(&str) -> Result<A>) -> Result<A> {
    param_opt
        .ok_or_else(|| error!("insufficient parameters"))
        .and_then(|value| parse_value(value))
        .map_err(|_| error!("you must give {}", name))
}

fn parse_data(param_opt: Option<&str>, name: &str) -> Result<ton::bytes> {
    parse_any(
        param_opt,
        &format!("{} in hex format", name),
        |value| Ok(ton::bytes(hex::decode(value)?))
    )
}

fn parse_int256(param_opt: Option<&str>, name: &str) -> Result<ton::int256> {
    parse_any(
        param_opt,
        &format!("{} in hex of base64 format", name),
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

fn parse_int(param_opt: Option<&str>, name: &str) -> Result<ton::int> {
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
    fn receive(answer: TLObject) -> std::result::Result<(), TLObject> {
        answer.downcast::<ton_api::ton::engine::validator::KeyHash>()
            .map(|key_hash| println!("received public key hash: {} {}",
                hex::encode(&key_hash.key_hash().0), base64::encode(&key_hash.key_hash().0)))
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
    fn receive(answer: TLObject) -> std::result::Result<(), TLObject> {
        answer.downcast::<ton_api::ton::engine::validator::Signature>()
            .map(|signature| println!("got signature: {} {}",
                hex::encode(&signature.signature().0), base64::encode(&signature.signature().0)))
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

/// Lite client
struct LiteClient{
    adnl: AdnlClient,
}

impl LiteClient {

    /// Connect to server
    async fn connect(config: &AdnlClientConfig) -> Result<Self> {
        Ok(Self {
            adnl: AdnlClient::connect(config).await?,
        })
    }

    /// Shutdown client
    async fn shutdown(self) -> Result<()> {
        self.adnl.shutdown().await
    }

    async fn command(&mut self, cmd: &str) -> Result<()> {
        let mut split = cmd.split_whitespace();
        let name = split.next().expect("takes_value set for COMMANDS");
        let query = command_send(name, split)?;
        let boxed = ControlQuery {
            data: ton::bytes(serialize(&query)?)
        };
        let answer = self.adnl.query(&TLObject::new(boxed)).await
            .map_err(|err| error!("Error receiving answer: {}", err))?;
        match answer.downcast::<ControlQueryError>() {
            Err(answer) => match command_receive(name, answer) {
                Err(answer) => fail!("Wrong response to {:?}: {:?}", query, answer),
                Ok(()) => Ok(())
            }
            Ok(error) => fail!("Error response to {:?}: {:?}", query, error),
        }
    }
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
            .short("g")
            .long("config")
            .help("config for console")
            .conflicts_with("ADDRESS")
            .conflicts_with("PUBLIC")
            .conflicts_with("KEY")
            .takes_value(true)
            .number_of_values(1)
            .required(true))
        .arg(Arg::with_name("ADDRESS")
            .short("a")
            .long("address")
            .help("server address")
            .conflicts_with("CONFIG")
            .takes_value(true)
            .required(true))
        .arg(Arg::with_name("PUBLIC")
            .short("p")
            .long("pub")
            .help("server public key")
            .conflicts_with("CONFIG")
            .takes_value(true)
            .required(true))
        .arg(Arg::with_name("KEY")
            .short("k")
            .long("key")
            .help("private key")
            .conflicts_with("CONFIG")
            .takes_value(true))
        .arg(Arg::with_name("COMMANDS")
            .short("c")
            .long("cmd")
            .help("schedule command")
            .takes_value(true)
            .number_of_values(1)
            .multiple(true))
        .arg(Arg::with_name("TIMEOUT")
            .short("t")
            .long("timeout")
            .help("timeout in batch mode")
            .takes_value(true)
            .number_of_values(1))
        .get_matches();

    let config = if let Some(config) = args.value_of("CONFIG") {
        std::fs::read_to_string(config).unwrap()
    } else {
        let mut map = serde_json::Map::new();
        map.insert("server_address".to_string(), args.value_of("ADDRESS").expect("required set for address").into());
        map.insert("server_key".to_string(), serde_json::json!({
            "type_id": 1209251014,
            "pub_key": std::fs::read_to_string(args.value_of("PUBLIC").expect("required set for public key")).unwrap()
        }).into());
        if let Some(client_key) = args.value_of("KEY") {
            map.insert("client_key".to_string(), serde_json::json!({
                "type_id": 1209251014,
                "pvt_key": std::fs::read_to_string(client_key).unwrap()
            }).into());
        }
        serde_json::json!(map).to_string()
    };
    let config = AdnlClientConfig::from_json(&config).unwrap();
    let timeout = match args.value_of("TIMEOUT") {
        Some(timeout) => u64::from_str(timeout).expect("timeout must be set in microseconds"),
        None => 0
    };
    let timeout = std::time::Duration::from_micros(timeout);
    let mut client = LiteClient::connect(&config).await.unwrap();
    if let Some(commands) = args.values_of("COMMANDS") {
        // batch mode - call commands and exit
        for command in commands {
            if let Err(err) = client.command(command.trim_matches('\"')).await {
                println!("Error executing comamnd: {}", err);
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
                command => {
                    println!("{}", command);
                    if let Err(err) = client.command(command).await {
                        println!("{}", err)
                    }
                }
            }
        }
    }
    client.shutdown().await.unwrap();
}
