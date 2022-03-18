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

use clap::{Arg, App};
use std::str::FromStr;
use ton_block::{
    BlockIdExt, Block, Deserializable, ShardStateUnsplit, McShardRecord
};
use ton_node::{
    collator_test_bundle::create_engine_allocated, 
    internal_db::{InternalDb, InternalDbConfig}
};
#[cfg(feature = "telemetry")]
use ton_node::collator_test_bundle::create_engine_telemetry;
use ton_types::{error, Result};

fn print_block(block: &Block, brief: bool) -> Result<()> {
    if brief {
        println!("{}", ton_block_json::debug_block(block.clone())?);
    } else {
        println!("{}", ton_block_json::debug_block_full(block)?);
    }
    Ok(())
}

fn print_state(state: &ShardStateUnsplit, brief: bool) -> Result<()> {
    if brief {
        println!("{}", ton_block_json::debug_state(state.clone())?);
    } else {
        println!("{}", ton_block_json::debug_state_full(state.clone())?);
    }
    Ok(())
}

async fn print_db_block(db: &InternalDb, block_id: BlockIdExt, brief: bool) -> Result<()> {
    println!("loading block: {}", block_id);
    let handle = db.load_block_handle(&block_id)?.ok_or_else(
        || error!("Cannot load block {}", block_id)
    )?;
    let block = db.load_block_data(&handle).await?;
    print_block(block.block(), brief)
}

async fn print_db_state(db: &InternalDb, block_id: BlockIdExt, brief: bool) -> Result<()> {
    println!("loading state: {}", block_id);
    let state = db.load_shard_state_dynamic(&block_id)?;
    print_state(state.state(), brief)
}

async fn print_shards(db: &InternalDb, block_id: BlockIdExt) -> Result<()> {
    println!("loading state: {}", block_id);
    let state = db.load_shard_state_dynamic(&block_id)?;
    if let Ok(shards) = state.shards() {
        shards.iterate_shards(|shard, descr| {
            let descr = McShardRecord::from_shard_descr(shard, descr);
            println!("before_merge: {} {}", descr.descr.before_merge, descr.block_id());
            Ok(true)
        })?;
    }
    Ok(())
}

// full BlockIdExt or masterchain seq_no
fn get_block_id(db: &InternalDb, id: &str) -> Result<BlockIdExt> {
    if let Ok(id) = BlockIdExt::from_str(id) {
        Ok(id)
    } else {
        let mc_seqno = u32::from_str(id)?;
        let handle = db.find_mc_block_by_seq_no(mc_seqno)?;
        Ok(handle.id().clone())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(Arg::with_name("PATH")
            .short("p")
            .long("path")
            .help("path to DB")
            .takes_value(true)
            .default_value("node_db")
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
        .arg(Arg::with_name("SHARDS")
            .short("r")
            .long("shards")
            .help("shard ids from master with seqno")
            .takes_value(true)
            .number_of_values(1))
        .arg(Arg::with_name("BOC")
            .short("c")
            .long("boc")
            .help("print containtment of bag of cells")
            .takes_value(true)
            .number_of_values(1))
        .arg(Arg::with_name("BRIEF")
            .short("i")
            .long("brief")
            .help("print brief info (block without messages and transactions, state without accounts) "))
        .get_matches();

    let brief = args.is_present("BRIEF");
    if let Some(path) = args.value_of("BOC") {
        let bytes = std::fs::read(path)?;
        if let Ok(block) = Block::construct_from_bytes(&bytes) {
            print_block(&block, brief)?;
        } else if let Ok(state) = ShardStateUnsplit::construct_from_bytes(&bytes) {
            print_state(&state, brief)?;
        }
    } else if let Some(db_dir) = args.value_of("PATH") {
        let db_config = InternalDbConfig { 
            db_directory: db_dir.to_string(), 
            cells_gc_interval_sec: 0
        };
        let db = InternalDb::new(
            db_config, 
            #[cfg(feature = "telemetry")]
            create_engine_telemetry(),
            create_engine_allocated()
        ).await?;
        if let Some(block_id) = args.value_of("BLOCK") {
            let block_id = get_block_id(&db, block_id)?;
            print_db_block(&db, block_id, brief).await?;
        }
        if let Some(block_id) = args.value_of("STATE") {
            let block_id = get_block_id(&db, block_id)?;
            print_db_state(&db, block_id, brief).await?;
        }
        if let Some(block_id) = args.value_of("SHARDS") {
            let block_id = get_block_id(&db, block_id)?;
            print_shards(&db, block_id).await?;
        }
    }
    Ok(())
}
