use adnl::{common::KeyOption, node::{AdnlNode, AdnlNodeConfig}};
use dht::DhtNode;
use overlay::OverlayNode;
use std::{env, ops::Deref};
use ton_node::config::TonNodeGlobalConfigJson;
use ton_types::{ error, Result };

const IP: &str = "0.0.0.0:4900";
const KEY_TAG: usize = 1;

fn scan(config: &str) -> Result<()> {

    let config = TonNodeGlobalConfigJson::from_json_file(config).map_err(
        |e| error!("Cannot read config from file {}: {}", config, e) 
    )?;
    let zero_state = config.zero_state()?;
    let zero_state = zero_state.file_hash;
    let dht_nodes =  config.get_dht_nodes_configs()?;

    let mut rt = tokio::runtime::Runtime::new()?;
    let config = AdnlNodeConfig::with_ip_address_and_key_type(
        IP, 
        KeyOption::KEY_ED25519, 
        KEY_TAG
    )?;
    let adnl = rt.block_on(AdnlNode::with_config(config))?;
    let dht = DhtNode::with_adnl_node(adnl.clone(), KEY_TAG)?;
    let overlay = OverlayNode::with_adnl_node_and_zero_state(
        adnl.clone(), 
        zero_state.as_slice(), 
        KEY_TAG
    )?;
    let overlay_id = overlay.calc_overlay_short_id(-1, 0x8000000000000000u64 as i64)?;
    rt.block_on(AdnlNode::start(&adnl, vec![dht.clone(), overlay.clone()]))?;
    for dht_node in dht_nodes.iter() {
        dht.add_peer(dht_node)?;
    }

    println!("Scanning DHT...");
    let mut iter = None;
    loop {
        let res = rt.block_on(DhtNode::find_overlay_nodes(&dht, &overlay_id, &mut iter))?;
        println!(
            "Found {} new nodes, searching more...", 
            dht.get_known_nodes(5000)?.len() - dht_nodes.len()
        );
        if res.is_empty() {
            break;
        }
    }
    let nodes = dht.get_known_nodes(5000)?;
    let mut count = nodes.len();
    if count > 0 {
        println!("---- Found DHT nodes:");
        for node in nodes {
            let mut skip = false;
            for dht_node in dht_nodes.iter() {
                if dht_node.id == node.id {
                    skip = true;
                    break;
                }
            } 
            if skip {
                continue;
            }
            let key = KeyOption::from_tl_public_key(&node.id)?;
            let adr = AdnlNode::parse_address_list(&node.addr_list)?.into_udp();
            println!(                         
                "{{\n    \
                    \"@type\": \"dht.node\",\n    \
                    \"id\": {{\n        \
                        \"@type\": \"pub.ed25519\",\n        \
                        \"key\": \"{}\"\n    \
                    }},\n    \
                    \"addr_list\": {{\n        \
                        \"@type\": \"adnl.addressList\",\n         \
                        \"addrs\": [\n            \
                            {{\n                \
                                \"@type\": \"adnl.address.udp\",\n                \
                                \"ip\": {},\n                \
                                \"port\": {}\n            \
                            }}\n        \
                        ],\n        \
                        \"version\": 0,\n        \
                        \"reinit_date\": 0,\n        \
                        \"priority\": 0,\n        \
                        \"expire_at\": 0\n    \
                    }},\n    \
                    \"version\": -1,\n    \
                    \"signature\": \"{}\"\n\
                }}{}",
                base64::encode(key.pub_key()?),
                adr.ip,
                adr.port,
                base64::encode(node.signature.deref()),
                if count > 1 {
                    ","
                } else {
                    ""
                }
            );
            count -= 1;
        }       
    } else {
        println!("---- No DHT nodes found");
    }
    Ok(())
} 

fn main() {
    let mut config = None;
    for arg in env::args().skip(1) {
        config = Some(arg);
        break;
    }
    let config = if let Some(config) = config {
         config
    } else {
         println!("Usage: dhtscan <path-to-global-config>");
         return
    };
    scan(&config).unwrap_or_else(|e| println!("DHT scanning error: {}", e))
}
