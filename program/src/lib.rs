mod create_pda_account;
mod entrypoint;
pub mod instruction;
pub mod processor;
pub mod state;

use solana_program::pubkey::Pubkey;

solana_program::declare_id!("F14xykzG2KNhVVLo6kVKQ6QPN8anVWUvrp7GdNPAkQm2"); // TODO

pub(crate) const REWARDS_VAULT_PDA_PREFIX: &[u8] = b"RewardsVault";

pub fn get_rewards_vault_address(vote_account_address: &Pubkey) -> Pubkey {
    get_rewards_vault_address_and_bump_seed(vote_account_address).0
}

pub(crate) fn get_rewards_vault_address_and_bump_seed(
    vote_account_address: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[REWARDS_VAULT_PDA_PREFIX, &vote_account_address.to_bytes()],
        &id(),
    )
}
