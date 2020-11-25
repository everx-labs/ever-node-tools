use adnl::common::KeyOption;
use ton_types::Result;
     
fn gen() -> Result<()> {
    let (private, public) = KeyOption::with_type_id(KeyOption::KEY_ED25519)?;
    println!("Keypair generated:");
    println!(                         
        "{{\n    \
             \"type_id\": {},\n    \
             \"pub_key\": \"{}\",\n    \
             \"pvt_key\": \"{}\"\n\
        }}",
        KeyOption::KEY_ED25519,
        base64::encode(public.pub_key()?),
        base64::encode(&private[..])
    );        
    Ok(())
} 

fn main() {
    gen().unwrap_or_else(|e| println!("Keypair generation error: {}", e))
}
