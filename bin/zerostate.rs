use clap::{Arg, App};
use serde_json::{Map, Value};
use ton_block::{
    Deserializable, Serializable, ShardIdent, ShardStateUnsplit, UnixTime32
};
use ton_types::{serialize_toc, Result, UInt256, HashmapType};

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
        let name = format!("{:x}.boc", descr.zerostate_file_hash);
        std::fs::write(name, &bytes)?;
        wc_zero_state.push(state);
        Ok(true)
    })?;
    extra.config.config_params.setref(12u32.serialize()?.into(), &wc_info.serialize()?)?;
    let ccvc = extra.config.catchain_config()?;
    let cur_validators = extra.config.validator_set()?;
    let (_validators, hash_short) = cur_validators.calc_subset(
        &ccvc, 
        ton_block::SHARD_FULL, 
        ton_block::MASTERCHAIN_ID, 
        0,
        UnixTime32(now)
    )?;
    extra.validator_info.validator_list_hash_short = hash_short;
    extra.validator_info.nx_cc_updated = true;
    extra.validator_info.catchain_seqno = 0;
    mc_zero_state.write_custom(Some(&extra))?;
    mc_zero_state.update_config_smc()?;
    let cell = mc_zero_state.serialize().unwrap();
    let bytes = serialize_toc(&cell).unwrap();
    let file_hash = UInt256::calc_file_hash(&bytes);
    let name = format!("{:x}.boc", file_hash);
    std::fs::write(name, &bytes).unwrap();

    // CHECK mc_zero_state
    let mc_zero_state = ShardStateUnsplit::construct_from_bytes(&bytes).expect("can't deserialize state");
    let extra = mc_zero_state.read_custom().expect("extra wasn't read from state").expect("extra must be in state");
    extra.config.config_params.iterate_slices(|ref mut key, ref mut param| {
        u32::construct_from(key).expect("index wasn't deserialized incorrectly");
        param.checked_drain_reference().expect("must contain reference");
        Ok(true)
    }).expect("somthing wrong with config");
    let prices = extra.config.storage_prices().expect("prices weren't read from config");
    for i in 0..prices.len().expect("prices len wasn't read") as u32 {
        prices.get(i).expect(&format!("prices description {} wasn't read", i));
    }

    let json = serde_json::json!({
        "zero_state": {
            "workchain": -1,
            "shard": -9223372036854775808i64,
            "seqno": 0,
            "root_hash": base64::encode(cell.repr_hash().as_slice()),
            "file_hash": base64::encode(&file_hash.as_slice()),
        }
    });

    let json = serde_json::to_string_pretty(&json)?;
    std::fs::write("config.json", &json)?;
    println!("{}", json);

    // check correctness
    // std::fs::write("new.json", ton_block_json::debug_state(mc_zero_state)?)?;

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
            .default_value("zero_state.json")
            .takes_value(true)
            .number_of_values(1))
        .get_matches();

    let file_name = args.value_of("INPUT").expect("required set for INPUT");
    let json = std::fs::read_to_string(file_name).unwrap();
    import_zerostate(&json).unwrap();
}
