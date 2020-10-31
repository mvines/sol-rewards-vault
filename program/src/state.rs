use {
    bytemuck::{Pod, Zeroable},
    solana_program::pubkey::Pubkey,
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, PartialEq, Eq)]
pub struct RewardsVaultState {
    pub original_withdraw_authority: Pubkey,
    pub rewards_authority: Pubkey,
}

impl RewardsVaultState {
    pub fn size_of() -> usize {
        std::mem::size_of::<Self>()
    }
}
