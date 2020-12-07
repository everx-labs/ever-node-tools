use clap::{Arg, App};
use serde_json::{Map, Value};
use ton_block::Serializable;

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
        .arg(Arg::with_name("OUTPUT")
            .short("o")
            .long("output")
            .help("output filename boc of zerostate")
            .required(true)
            .takes_value(true)
            .number_of_values(1))
        .get_matches();

    let file_name = args.value_of("INPUT").expect("required set for INPUT");
    let json = std::fs::read_to_string(file_name).unwrap();
    let map = serde_json::from_str::<Map<String, Value>>(&json).unwrap();
    let mc_zero_state = ton_block_json::parse_state(&map).unwrap();
    let file_name = args.value_of("OUTPUT").expect("required set for OUTPUT");
    mc_zero_state.write_to_file(file_name).unwrap();
    let result = ton_block_json::debug_state(mc_zero_state.clone()).unwrap();
    if json != result {
        std::fs::write("new.json", ton_block_json::debug_state(mc_zero_state).unwrap().as_bytes()).unwrap();
        panic!("generated zerostate does not correspond to input json")
    }
}
