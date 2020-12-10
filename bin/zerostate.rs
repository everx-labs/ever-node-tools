use clap::{Arg, App};
use serde_json::{Map, Value};
use ton_block::{ShardStateUnsplit, Serializable, ShardIdent};
use ton_types::{serialize_toc, Result, UInt256};

fn import_zerostate(json: &str) -> Result<()> {
    let map = serde_json::from_str::<Map<String, Value>>(&json)?;
    let mut mc_zero_state = ton_block_json::parse_state(&map)?;
    let now = mc_zero_state.gen_time();
    // let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as u32;
    // mc_zero_state.set_gen_time(now);
    let mut extra = mc_zero_state.read_custom()?.expect("must be in mc state");
    let mut wc_info = extra.config.workchains()?;
    let mut wc_zero_state = vec![];
    wc_info.clone().iterate_with_keys(|workchain_id, mut descr| {
        let shard = ShardIdent::with_tagged_prefix(workchain_id, ton_block::SHARD_FULL)?;
        // generate empty shard state and set desired fields
        let mut state = ShardStateUnsplit::with_ident(shard);
        state.set_gen_time(now);
        state.set_global_id(mc_zero_state.global_id());
        state.set_min_ref_mc_seqno(u32::MAX);

        let cell = state.serialize()?;
        descr.zerostate_root_hash = cell.repr_hash();
        let bytes = ton_types::serialize_toc(&cell)?;
        descr.zerostate_file_hash = UInt256::calc_file_hash(&bytes);
        wc_info.set(&workchain_id, &descr)?;
        let name = format!("basestate{}.boc", workchain_id);
        std::fs::write(name, &bytes)?;
        wc_zero_state.push(state);
        Ok(true)
    })?;
    extra.config.config_params.set(12u32.serialize()?.into(), &wc_info.serialize()?.into())?;
    mc_zero_state.write_custom(Some(&extra))?;
    let cell = mc_zero_state.serialize().unwrap();
    let bytes = serialize_toc(&cell).unwrap();
    std::fs::write("zerostate.boc", &bytes).unwrap();

    let json = serde_json::json!({
        "zero_state": {
            "workchain": -1,
            "shard": -9223372036854775808i64,
            "seqno": 0,
            "root_hash": base64::encode(cell.repr_hash().as_slice()),
            "file_hash": base64::encode(UInt256::calc_file_hash(&bytes).as_slice()),
        }
    });

    let json = serde_json::to_string_pretty(&json)?;
    std::fs::write("config.json", &json)?;
    println!("{}", json);

    Ok(())
}

fn main() {
    let args = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(Arg::with_name("INPUT")
            .short("i")
            .long("input")
            .help("input json filename with masterchain zerostate")
            .required(true)
            .takes_value(true)
            .number_of_values(1))
        .get_matches();

    let file_name = args.value_of("INPUT").expect("required set for INPUT");
    let json = std::fs::read_to_string(file_name).unwrap();
    import_zerostate(&json).unwrap();
}
