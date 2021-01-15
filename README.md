# ton-node-tools

This repository contains a collection of tools used to manage the TON Labs Rust Node.

# Console

This tool serves the purpose of generating election requests for the Rust Node. The tool is compatible with [TONOS-CLI](https://github.com/tonlabs/tonos-cli) and allows to perform all actions necessary to obtain a signed election request.

## How to use

### Command syntax

```bash
console -C config.json -c "commamd with parameters" -c "another command" -t timeout
```

Where

`config.json` - path to configuration file

`commamd with parameters`/ `another command` – any of the supported console commands with necessary parameters

`timeout` – command timeout in seconds

Configuration file should be created manually and have the following format:

```json
{
		"config": {
				"server_address": "127.0.0.1:4924",
				"server_key": {
						"type_id": 1209251014,
						"pub_key": "cujCRU4rQbSw48yHVHxQtRPhUlbo+BuZggFTQSu04Y8="
				},
				"client_key": {
						"type_id": 1209251014,
						"pvt_key": "oEivbTDjSOSCgooUM0DAS2z2hIdnLw/PT82A/OFLDmA="
				}
		},
		"wallet_id": "-1:af17db43f40b6aa24e7203a9f8c8652310c88c125062d1129fe883eaa1bd6763",
		"max_factor": 2.7
}
```

Where

`server_address` – address and port of the node.

`server_key` – structure containing server public key. Can be generated with keygen tool.

`client_key` – structure containing client private key. Can be generated with keygen tool.

`type_id` – key type, indicating ed25519 is used. Should not be changed.

`wallet_id` – validator wallet address.

`max_factor` – [max_factor](https://docs.ton.dev/86757ecb2/p/456977-validator-elections) stake parameter (maximum ratio allowed between your stake and the minimal
 validator stake in the elected validator group), should be ≥ 1
 

### Commands

#### election-bid

**`election-bid`** - obtains required information from the blockchain, generates all the necessary keys for validator, prepares the message in predefined format, asks to sign it and sends to the blockchain.

params:

• `election-start` - unixtime of election start.

• `election-end` - unixtime of election end.

• `filename` - filename with path to save body of message ("validator-query.boc" by default)

Example:

```bash
console -c "election-bid 1608205174 1608288600"
```

Command calls all other necessary subcommands automatically. Election request is written to file.


#### recover_stake

**`recover_stake`** – recovers all or part of the validator stake from elector.

params:

• `filename` - filename with path to save body of message ("recover-query.boc" by default)

Example:

```bash
console -c "recover_stake"
```

#### newkey

**`newkey`** - generates new key pair on server.

Command has no parameters.

Returns ed25519 hash of public key in hex and base64 format.

Example:

```bash
console -c "newkey"
```

#### exportpub

**`exportpub`** - exports public key by key hash.

params:

• `key_hash` - ed25519 hash of public key in hex or base64 format.

Returns public_key - ed25519 public key in hex and base64 format.

Example:

```bash
console -c "exportpub 4374376452376543"
```

#### sign

**`sign`** - signs bytestring with private key.

params:

• `key_hash` - ed25519 hash of public key in hex or base64 format.

• `data` - data in hex or base64 format.

Example:

```bash
console -c "sign 4374376452376543 af17db43f40b6aa24e7203a9f8c8652310c88c125062d1129f"
```

#### addpermkey

**`addpermkey`** - adds validator permanent key

params:

• `key_hash` - ed25519 hash of public key in hex or base64 format.

• `election-date` - election start in unixtime.

• `expire-at`- time the key expires and is deleted from node, in unixtime.

Example:

```bash
console -c "addpermkey 4374376452376543 1608205174 1608288600"
```

#### addtempkey

**`addtempkey`** - adds validator temporary key.

params:

• `perm_key_hash` - ed25519 hash of permanent public key in hex or base64 format.

• `key_hash` - ed25519 hash of public key in hex or base64 format.

• `expire-at` - time the key expires and is deleted from node, in unixtime.

Example:

```bash
console -c "addtempkey 4374376452376543 6783978551824553 1608288600"
```

#### addvalidatoraddr

**`addvalidatoraddr`** - adds validator ADNL address.

params:

• `perm_key_hash` - ed25519 hash of permanent public key in hex or base64 format.

• `key_hash` - ed25519 hash of public key in hex or base64 format.

• `expire-at`- time the ADNL address expires and is deleted from node, in unixtime.

Example:

```bash
console -c "addvalidatoraddr 4374376452376543 6783978551824553 1608288600"
```

#### addadnl

**`addadnl`** – sets key as ADNL address.

params:

• `perm_key_hash` - ed25519 hash of permanent public key in hex or base64 format.

• `key_hash` - ed25519 hash of public key in hex or base64 format.

• `expire-at` - time the ADNL address expires and is deleted from node, in unixtime.

Example:

```bash
console -c "addadnl 4374376452376543 6783978551824553 1608288600"
```


# zerostate

This tool generates config and zerostate for network launch from json zerostate file.

## How to use

```bash
zerostate -i zerostate.json
```

Where

`zerostate.json` – is the zerostate file.

# keygen

This tool generates an ed25519 key and prints it out in different formats.

## How to use

```bash
keygen
```

Command has no parameters.

# gendht

This tool generates the node DHT record, for example, for the purposes of adding it to the global blockchain config.

## How to use

```bash
gendht ip:port pvt_key
```

Where

`ip:port` – Node IP address and port.

`pvt_key` – Node private key.

Example:

```bash
gendht 51.210.114.123:30303 ABwHd2EavvLJk789BjSF3OJBfX6c26Uzx1tMbnLnRTM=
```

# dhtscan

This tool scans DHT for node records.

## How to use

```bash
dhtscan [--jsonl] [--overlay] [--workchain0] path-to-global-config
```

Where

`--jsonl` – optional flag that sets the output as single line json. Default output is multiline json.

`--overlay` – optional flag to search for overlay nodes.

`--workchain0` – optional flag to search both in masterchain and basechain. By default only masterchain is searched.

`path-to-global-config` – path to global config file.

# print

This tool prints a state or block from the database.

## How to use

```bash
print -d path [-s state_id] [-b block_id]
```

Where

`path` – path to node database.

`block_id` – id of the block to be printed.

`state_id` – id of the state to be printed.

