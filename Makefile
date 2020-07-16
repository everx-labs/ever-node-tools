all: Makefile 
	@cargo run --release --bin keygen

dht: Makefile 
	@cargo run --release --bin dhtscan ton-global.config-mainet.json

