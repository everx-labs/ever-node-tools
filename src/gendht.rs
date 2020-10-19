use adnl::{adnl_node_test_key, adnl_node_test_config, common::KeyOption, node::{AdnlNode, AdnlNodeConfig}};
use dht::DhtNode;
use std::{env, ops::Deref};
use ton_types::{Result};

fn gen(ip: &str) -> Result<()> {
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    let (_, dht_key) = KeyOption::with_type_id(KeyOption::KEY_ED25519)?;
    let dht_key_enc = base64::encode(dht_key.pvt_key()?);
    let config = AdnlNodeConfig::from_json(
       adnl_node_test_config!(ip, adnl_node_test_key!(1 as usize, dht_key_enc)),
       true
    ).unwrap();
    let adnl = rt.block_on(AdnlNode::with_config(config)).unwrap();
    let dht = DhtNode::with_adnl_node(adnl.clone(), 1 as usize).unwrap();
    let node = dht.get_signed_node().unwrap();
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
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
} 

fn main() {
    let mut ip : std::string::String = "0.0.0.0:30303".to_string();
    let mut check = false;
    for arg in env::args().skip(1) {
        ip = arg;
        check = true;
    }
    if check == false {
        println!("Usage: genconfig <ip:port>");
        return
    };
    gen(&ip).unwrap_or_else(|e| println!("gen error: {}", e))
}
