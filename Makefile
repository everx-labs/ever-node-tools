all: Makefile 
	@cargo build --release

adnl: Makefile 
	@cargo run --release --bin adnl_resolve YTgEDtd9WyqPONsozHxlI5X1Jj+pHGjQM7yIn82b+Jo= ..\configs\ton-global.config-mainet.json
	
key: Makefile 
	@cargo run --release --bin keygen

keyid: Makefile 
#	@cargo run --release --bin keyid pvt 3pdYMu6fo8IMq1PBaVDHKrFSBrSK/M/+85CXIwCkz5g=
#	@cargo run --release --bin keyid pvt 9Pk32/BmvPGbhqXVyXwHruhVRZzoqygEsCII2vdPt1k=
	@cargo run --release --bin keyid pvt tLO00fTB/83cSd744Gp43fmJG896HN/WnUmN/8RlGZE=

dht: Makefile 
	@cargo run --release --bin dhtscan ton-global.config-mainet.json --jsonl

tst: Makefile 
	@cargo run --release --bin notests ../notests

ping: Makefile 
	@cargo run --release --bin adnl_ping p+9/xO85UFX58sWtPf0JaQMcq5khjKAzCqTYjnAdKWE= 51.210.214.159:30307 ..\configs\ton-global.config-mainet.json
