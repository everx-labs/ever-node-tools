all: Makefile 
	@cargo build --release

key: Makefile 
	@cargo run --release --bin keygen

dht: Makefile 
	@cargo run --release --bin dhtscan ton-global.config-mainet.json --jsonl

tst: Makefile 
	@cargo run --release --bin notests ../notests