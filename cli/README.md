# Sol Rewards Vault Command-line Interface

```
$ sol-rewards-vault-cli --help
sol-rewards-vault-cli 0.1.0


USAGE:
    sol-rewards-vault-cli [OPTIONS] <SUBCOMMAND>

OPTIONS:
    -C, --config <PATH>          Configuration file to use
        --fee_payer <KEYPAIR>    Specify the fee-payer account
    -h, --help                   Print help information
    -u, --url <URL>              JSON RPC URL for the cluster [default: value from configuration
                                 file]
    -v, --verbose                Show additional information
    -V, --version                Print version information

SUBCOMMANDS:
    enter       Place a vote account in its rewards vault
    help        Print this message or the help of the given subcommand(s)
    leave       Remove a vote account from its rewards vault
    withdraw    Claim epoch rewards earned by a vote account residing in its rewards vault
```

## Quick Start
1. Install Rust from https://rustup.rs/
1. cargo run


