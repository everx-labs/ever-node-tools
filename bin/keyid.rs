use adnl::common::KeyOption;
use std::{convert::TryInto, env};
use ton_types::{error, fail, Result};
     
fn compute(typ: &str, key: &str) -> Result<()> {
    let key_bin = base64::decode(key)?;
    let key_bin: [u8; 32] = key_bin.try_into().map_err(
        |_| error!("Cannot decode key properly") 
    )?;
    let key = if typ.to_lowercase().as_str() == "pub" {
        println!("Public key: {}", key);
        KeyOption::from_type_and_public_key(KeyOption::KEY_ED25519, &key_bin)
    } else if typ.to_lowercase().as_str() == "pvt" {
        println!("Private key: {}", key);
        let (_, key) = KeyOption::from_type_and_private_key(KeyOption::KEY_ED25519, &key_bin)?;
        key
    } else {
        fail!("Wrong key type: expected pub|pvt, found {}", typ)
    };
    println!("Key id: {}", base64::encode(key.id().data()));
    Ok(())
} 

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        println!("Usage: keyid pub|pvt <key in base64>");
        return
    };
    compute(&args[1], &args[2]).unwrap_or_else(|e| println!("Key ID computing error: {}", e))
}
