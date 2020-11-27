use adnl::common::serialize;
use adnl::client::{AdnlClient, AdnlClientConfig};
use ton_api::{
    ton::{
        self, TLObject, 
        engine::validator::ControlQueryError,
        rpc::engine::validator::{AddValidatorAdnlAddress, ControlQuery, GenerateKeyPair},
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
    fn receive(answer: TLObject) -> std::result::Result<(), TLObject>;
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
    AddValidatorAdnlAddr, "addvalidatoraddr", "addvalidatoraddr <permkeyhash> <keyhash> <expireat>\tadd validator ADNL addr"
}

impl SendReceive for NewKeypair {
    fn send<'a>(_params: impl Iterator<Item = &'a str>) -> Result<TLObject> {
        Ok(TLObject::new(GenerateKeyPair))
    }
    fn receive(answer: TLObject) -> std::result::Result<(), TLObject> {
        answer.downcast::<ton_api::ton::engine::validator::KeyHash>()
            .map(|key_hash| println!("received public key hash: {}", base64::encode(&key_hash.key_hash().0)))
    }
}

impl SendReceive for AddValidatorAdnlAddr {
    fn send<'a>(mut params: impl Iterator<Item = &'a str>) -> Result<TLObject> {
        let permanent_key_hash =  ton::int256(params.next()
            .ok_or_else(|| error!("insufficient parameters"))
            .and_then(|hash| Ok(base64::decode(hash)?))
            .and_then(|hash| Ok(hash.as_slice().try_into()?))
            .map_err(|_| error!("you must give permanent_key_hash in format"))?
        );
        let key_hash =  ton::int256(params.next()
            .ok_or_else(|| error!("insufficient parameters"))
            .and_then(|hash| Ok(base64::decode(hash)?))
            .and_then(|hash| Ok(hash.as_slice().try_into()?))
            .map_err(|_| error!("you must give key_hash in base64 format"))?
        );
        let ttl = params.next()
            .ok_or_else(|| error!("insufficient parameters"))
            .and_then(|ttl| Ok(ton::int::from_str(ttl)?))
            .map_err(|_| error!("you must give expire_at"))?;
        Ok(TLObject::new(AddValidatorAdnlAddress {
            permanent_key_hash,
            key_hash,
            ttl
        }))
    }
    fn receive(answer: TLObject) -> std::result::Result<(), TLObject> {
        answer.downcast::<ton_api::ton::engine::validator::KeyHash>()
            .map(|key_hash| println!("received public key hash: {}", base64::encode(&key_hash.key_hash().0)))
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
        match self.adnl.query(&TLObject::new(boxed)).await?.downcast::<ControlQueryError>() {
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
        .arg(Arg::with_name("ADDRESS")
            .short("a")
            .long("address")
            .help("server address")
            .takes_value(true)
            .required(true))
        .arg(Arg::with_name("PUBLIC")
            .short("p")
            .long("pub")
            .help("server public key")
            .takes_value(true)
            .required(true))
        .arg(Arg::with_name("KEY")
            .short("k")
            .long("key")
            .help("private key")
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

    let config = AdnlClientConfig::with_params(
        args.value_of("ADDRESS").expect("required set for address"),
        args.value_of("PUBLIC").expect("required set for public key"),
        args.value_of("KEY"),
        Default::default()
    ).unwrap();
    let mut client = LiteClient::connect(&config).await.unwrap();
    if let Some(commands) = args.values_of("COMMANDS") {
        for command in commands {
            client.command(command.trim_matches('\"')).await.unwrap();
        }
    } else {
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
