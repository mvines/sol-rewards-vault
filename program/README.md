# DANGER: THIS PROGRAM AS NOT BEEN AUDITED

## Sol Rewards Vault Program
By default the vote account withdraw authority is used to withdraw epoch
rewards. However that authority is overloaded and is also used for other rarely
used vote account *management* operations, such as updating the vote account
commission and validator identity.

This program enables a vote account to be locked down such that only epoch
rewards may be withdrawn by a keypair other than the critically important
vote account withdraw authority.

Should this "rewards" keypair ever be compromised, the vote account itself
remains safe and a new rewards keypair may be installed to regain control.

### Program Design
For each vote account, the `RewardsVaultInstruction::Enter` instruction creates
a unique PDA that holds the original withdraw authority and the requested
rewards authority. The PDA itself is then set as the vote account's withdraw authority. In essence
the program takes custody of the vote account.


The `RewardsVaultInstruction::Leave` instruction performs the inverse of the
`Enter` instruction by restoring the original withdraw authority and deallocating
the unique PDA for the vote account.

While within the vault, the `RewardsVaultInstruction::ClaimRewards` instruction
ensures the reward authority is a signer and then invokes the vote program with
the PDA as signer to effect the withdrawal.

### Usage
See the `sol-rewards-vault-cli` command-line program

### Development
#### Environment Setup
1. Install Rust from https://rustup.rs/
2. Install Solana from https://docs.solana.com/cli/install-solana-cli-tools#use-solanas-install-tool

#### Build and test the program compiled for BPF
```
$ cargo build-bpf
$ cargo test-bpf
```
