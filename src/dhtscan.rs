use adnl::{common::KeyOption, node::{AdnlNode, AdnlNodeConfig}};
use dht::DhtNode;
use overlay::OverlayNode;
use std::{env, ops::Deref};
use ton_node::config::TonNodeGlobalConfigJson;
use ton_types::{error, fail, Result};

const IP: &str = "0.0.0.0:4900";
const KEY_TAG: usize = 1;

fn scan(config: &str, jsonl: bool) -> Result<()> {

    let config = TonNodeGlobalConfigJson::from_json_file(config).map_err(
        |e| error!("Cannot read config from file {}: {}", config, e) 
    )?;
    let zero_state = config.zero_state()?;
    let zero_state = zero_state.file_hash;
    let dht_nodes = config.get_dht_nodes_configs()?;

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
        if dht.add_peer(dht_node)?.is_none() {
            fail!("Invalid DHT peer {:?}", dht_node)
        }
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
    if nodes.len() > dht_nodes.len() {
        println!("---- Found DHT nodes:");
        for node in nodes {
            let mut skip = false;
            for dht_node in dht_nodes.iter() {
                if dht_node.id != node.id {
                    if let Ok(true) = dht.ping(dht_node.id).await {
                        continue;
                    }
                }
                skip = true;
                break;
            } 
            if skip {
                continue;
            }
            let key = KeyOption::from_tl_public_key(&node.id)?;
            let adr = AdnlNode::parse_address_list(&node.addr_list)?.into_udp();
            let json = serde_json::json!(
                {
                    "@type": "dht.node",
                    "id": {
                        "@type": "pub.ed25519",
                        "key": base64::encode(key.pub_key()?)
                    },
                    "addr_list": {
                        "@type": "adnl.addressList",
                        "addrs": [
                            {
                                "@type": "adnl.address.udp",
                                "ip": adr.ip,
                                "port": adr.port
                            }
                        ],
                        "version": node.addr_list.version,
                        "reinit_date": node.addr_list.reinit_date,
                        "priority": node.addr_list.priority,
                        "expire_at": node.addr_list.expire_at
                    },
                    "version": node.version,
                    "signature": base64::encode(node.signature.deref())
                }
            ); 
            println!(
                "{},", 
                if jsonl {
                    serde_json::to_string(&json)?
                } else { 
                    serde_json::to_string_pretty(&json)?
                }
            );
        }       
    } else {
        println!("---- No DHT nodes found");
    }
    Ok(())
} 

fn main() {
    let mut config = None;
    let mut jsonl = false;
    for arg in env::args().skip(1) {
        if arg == "--jsonl" {
            jsonl = true
        } else {
            config = Some(arg)
        }
    }
    let config = if let Some(config) = config {
        config
    } else {
        println!("Usage: dhtscan [--jsonl] <path-to-global-config>");
        return
    };
    scan(&config, jsonl).unwrap_or_else(|e| println!("DHT scanning error: {}", e))
}
