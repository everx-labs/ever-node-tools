use adnl::{common::{KeyId, KeyOption}, node::{AdnlNode, AdnlNodeConfig}};
use dht::DhtNode;
use std::{convert::TryInto, env, fs::File, io::BufReader};
use ton_node::config::TonNodeGlobalConfigJson;
use ton_types::{error, fail, Result};

include!("../common/src/test.rs");

const IP: &str = "0.0.0.0:4191";
const KEY_TAG: usize = 1;

fn scan(adnlid: &str, cfgfile: &str) -> Result<()> {

    let file = File::open(cfgfile)?;
    let reader = BufReader::new(file);
    let config: TonNodeGlobalConfigJson = serde_json::from_reader(reader).map_err(
        |e| error!("Cannot read config from file {}: {}", cfgfile, e) 
    )?;
    let dht_nodes = config.get_dht_nodes_configs()?;
    let rt = tokio::runtime::Runtime::new()?;
    let (_, config) = AdnlNodeConfig::with_ip_address_and_key_type(
        IP, 
        KeyOption::KEY_ED25519, 
        vec![KEY_TAG]
    )?;
    let adnl = rt.block_on(AdnlNode::with_config(config))?;
    let dht = DhtNode::with_adnl_node(adnl.clone(), KEY_TAG)?;
    rt.block_on(AdnlNode::start(&adnl, vec![dht.clone()]))?;

    let mut preset_nodes = Vec::new();
    for dht_node in dht_nodes.iter() {
        if let Some(key) = dht.add_peer(dht_node)? {
            preset_nodes.push(key)
        } else {
            fail!("Invalid DHT peer {:?}", dht_node)
        }
    }

    println!("Scanning DHT...");
    for node in preset_nodes.iter() {
        rt.block_on(dht.find_dht_nodes(node))?;
    }

    let keyid = KeyId::from_data((&base64::decode(adnlid)?[..32]).try_into()?);
    println!("Searching DHT for {}...", keyid);
    loop {
        if let OK((ip, key)) = rt.block_on(DhtNode::find_address(&dht, &keyid)) {
            println!("Found {} / {}", ip, key.id());
            return Ok(())
        } else {
            println!("Not found yet, next iteration...");
        }
    }

}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        println!("Usage: adnl_resolve <adnl-id> <path-to-global-config>");
        return
    };
    init_log("./common/config/log_cfg.yml");
    scan(args[1].as_str(), args[2].as_str()).unwrap_or_else(
        |e| println!("ADNL resolving error: {}", e)
    )
}
