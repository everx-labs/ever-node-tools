use clap::{Arg, App};
use std::str::FromStr;
use ton_block::{AccountIdPrefixFull, BlockIdExt};
use ton_node::db::{InternalDb, InternalDbConfig, InternalDbImpl};
use ton_types::Result;

async fn print_block(db: &InternalDbImpl, block_id: BlockIdExt) -> Result<()> {
    println!("loading block: {}", block_id);
    let handle = db.load_block_handle(&block_id)?;
    let block = db.load_block_data(&handle).await?;
    println!("{}", ton_block_json::debug_block(block.block().clone())?);
    Ok(())
}

async fn print_state(db: &InternalDbImpl, block_id: BlockIdExt) -> Result<()> {
    println!("loading state: {}", block_id);
    let state = db.load_shard_state_dynamic(&block_id)?;
    println!("{}", ton_block_json::debug_state(state.shard_state().clone())?);
    Ok(())
}

// full BlockIdExt or masterchain seq_no
fn get_block_id(db: &InternalDbImpl, id: &str) -> Result<BlockIdExt> {
    if let Ok(id) = BlockIdExt::from_str(id) {
        Ok(id)
    } else {
        let mc_seqno = u32::from_str(id)?;
        let acc_pfx = AccountIdPrefixFull::any_masterchain();
        let handle = db.find_block_by_seq_no(&acc_pfx, mc_seqno)?;
        Ok(handle.id().clone())
    }
}

#[tokio::main]
async fn main() {
    let args = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(Arg::with_name("PATH")
            .short("p")
            .long("path")
            .help("path to DB")
            .takes_value(true)
            .number_of_values(1))
        .arg(Arg::with_name("BLOCK")
            .short("b")
            .long("block")
            .help("print block")
            .takes_value(true)
            .number_of_values(1))
        .arg(Arg::with_name("STATE")
            .short("s")
            .long("state")
            .help("print state")
            .takes_value(true)
            .number_of_values(1))
        .get_matches();

    let db_dir = args.value_of("PATH").unwrap_or("db_node");
    let db_config = InternalDbConfig { db_directory: db_dir.to_string() };
    let db = InternalDbImpl::new(db_config).await.unwrap();

    if let Some(block_id) = args.value_of("BLOCK") {
        let block_id = get_block_id(&db, block_id).unwrap();
        print_block(&db, block_id).await.unwrap();
    }
    if let Some(block_id) = args.value_of("STATE") {
        let block_id = get_block_id(&db, block_id).unwrap();
        print_state(&db, block_id).await.unwrap();
    }
}
