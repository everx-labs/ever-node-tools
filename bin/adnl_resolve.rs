/*
* Copyright (C) 2019-2021 TON Labs. All Rights Reserved.
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

    let mut nodes = Vec::new();
    let mut bad_nodes = Vec::new();
    for dht_node in dht_nodes.iter() {
        if let Some(key) = dht.add_peer(dht_node)? {
            nodes.push(key)
        } else {
            fail!("Invalid DHT peer {:?}", dht_node)
        }
    }

    let keyid = KeyId::from_data((&base64::decode(adnlid)?[..32]).try_into()?);
    let mut index = 0;
    loop {
        println!("Searching DHT for {}...", keyid);
        if let Ok((ip, key)) = rt.block_on(DhtNode::find_address(&dht, &keyid)) {
            println!("Found {} / {}", ip, key.id());
            return Ok(())
        }
        if index >= nodes.len() {
            nodes.clear();
            for dht_node in dht.get_known_nodes(10000)?.iter() {
                if let Some(key) = dht.add_peer(dht_node)? {
                    if !bad_nodes.contains(&key) {
                        nodes.push(key)
                    }
                }
            } 
            if nodes.is_empty() {
                fail!("No good DHT peers")
            }
            index = 0;
        }
        println!(
            "Not found yet, scanning more DHT nodes from {} ({} of {}) ...", 
            nodes[index], 
            index, 
            nodes.len()
        );
        if !rt.block_on(dht.find_dht_nodes(&nodes[index]))? {
            println!("DHT node {} is non-responsive", nodes[index]); 
            bad_nodes.push(nodes.remove(index))
        } else {
            index += 1
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
