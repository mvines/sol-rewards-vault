use {
    crate::{
        create_pda_account::create_pda_account, get_rewards_vault_address_and_bump_seed, id,
        instruction::RewardsVaultInstruction, state::RewardsVaultState,
    },
    bytemuck::Zeroable,
    solana_program::{
        account_info::{next_account_info, AccountInfo},
        entrypoint::ProgramResult,
        msg,
        program::invoke_signed,
        program_error::ProgramError,
        pubkey::Pubkey,
        rent::Rent,
        sysvar::Sysvar,
        vote::state::VoteAuthorize,
    },
};

pub(crate) fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    assert_eq!(*program_id, id());

    let instruction = RewardsVaultInstruction::try_from(
        *instruction_data
            .first()
            .ok_or(ProgramError::InvalidInstructionData)?,
    )
    .map_err(|_| ProgramError::InvalidInstructionData)?;

    msg!("Instruction: {:?}", instruction);

    let account_info_iter = &mut accounts.iter();
    let vault_info = next_account_info(account_info_iter)?;
    let vote_account_info = next_account_info(account_info_iter)?;

    let vault_address_bump_seed = {
        let (vault_address, vault_address_bump_seed) =
            get_rewards_vault_address_and_bump_seed(vote_account_info.key);
        if vault_address != *vault_info.key {
            return Err(ProgramError::InvalidArgument);
        }
        vault_address_bump_seed
    };

    let vault_account_signer_seeds: &[&[_]] = &[
        crate::REWARDS_VAULT_PDA_PREFIX,
        &vote_account_info.key.to_bytes(),
        &[vault_address_bump_seed],
    ];

    match instruction {
        RewardsVaultInstruction::Enter => {
            let funder_info = next_account_info(account_info_iter)?;
            let withdraw_authority_info = next_account_info(account_info_iter)?;
            let rewards_authority_info = next_account_info(account_info_iter)?;
            let system_program_info = next_account_info(account_info_iter)?;
            let vote_program_info = next_account_info(account_info_iter)?;
            let clock_sysvar_info = next_account_info(account_info_iter)?;

            create_pda_account(
                funder_info,
                &Rent::get()?,
                /*space = */ RewardsVaultState::size_of(),
                &id(),
                system_program_info,
                vault_info,
                vault_account_signer_seeds,
            )?;

            {
                let mut vault_data = vault_info.try_borrow_mut_data()?;
                let vault_state =
                    bytemuck::try_from_bytes_mut::<RewardsVaultState>(&mut vault_data)
                        .map_err(|_| ProgramError::InvalidAccountData)?;

                if *vault_state != RewardsVaultState::zeroed() {
                    return Err(ProgramError::AccountAlreadyInitialized);
                }

                *vault_state = RewardsVaultState {
                    original_withdraw_authority: *withdraw_authority_info.key,
                    rewards_authority: *rewards_authority_info.key,
                };
            }

            invoke_signed(
                &solana_program::vote::instruction::authorize_checked(
                    vote_account_info.key,
                    withdraw_authority_info.key,
                    vault_info.key,
                    VoteAuthorize::Withdrawer,
                ),
                &[
                    vote_account_info.clone(),
                    withdraw_authority_info.clone(),
                    vault_info.clone(),
                    vote_program_info.clone(),
                    clock_sysvar_info.clone(),
                ],
                &[vault_account_signer_seeds],
            )
        }
        RewardsVaultInstruction::Leave => {
            let refunder_info = next_account_info(account_info_iter)?;
            let withdraw_authority_info = next_account_info(account_info_iter)?;
            let vote_program_info = next_account_info(account_info_iter)?;
            let clock_sysvar_info = next_account_info(account_info_iter)?;

            {
                let mut vault_data = vault_info.try_borrow_mut_data()?;
                let vault_state =
                    bytemuck::try_from_bytes_mut::<RewardsVaultState>(&mut vault_data)
                        .map_err(|_| ProgramError::InvalidAccountData)?;

                if vault_state.original_withdraw_authority != *withdraw_authority_info.key {
                    return Err(ProgramError::MissingRequiredSignature);
                }

                *vault_state = RewardsVaultState::zeroed();
            }

            invoke_signed(
                &solana_program::vote::instruction::authorize_checked(
                    vote_account_info.key,
                    vault_info.key,
                    withdraw_authority_info.key,
                    VoteAuthorize::Withdrawer,
                ),
                &[
                    vote_account_info.clone(),
                    vault_info.clone(),
                    withdraw_authority_info.clone(),
                    vote_program_info.clone(),
                    clock_sysvar_info.clone(),
                ],
                &[vault_account_signer_seeds],
            )?;

            {
                **refunder_info.try_borrow_mut_lamports()? += vault_info.lamports();
                **vault_info.try_borrow_mut_lamports()? = 0;
            }

            Ok(())
        }
        RewardsVaultInstruction::WithdrawRewards => {
            let rewards_recipient = next_account_info(account_info_iter)?;
            let rewards_authority_info = next_account_info(account_info_iter)?;
            let vote_program_info = next_account_info(account_info_iter)?;

            {
                let vault_data = vault_info.try_borrow_data()?;
                let vault_state = bytemuck::try_from_bytes::<RewardsVaultState>(&vault_data)
                    .map_err(|_| ProgramError::InvalidAccountData)?;

                if vault_state.rewards_authority != *rewards_authority_info.key
                    || !rewards_authority_info.is_signer
                {
                    return Err(ProgramError::MissingRequiredSignature);
                }
            }

            let minimum_balance = Rent::get()?.minimum_balance(vote_account_info.data_len());
            let lamports = vote_account_info
                .lamports()
                .checked_sub(minimum_balance)
                .ok_or(ProgramError::InvalidInstructionData)?;

            msg!("Withdrawing {} lamports", lamports);

            invoke_signed(
                &solana_program::vote::instruction::withdraw(
                    vote_account_info.key,
                    vault_info.key,
                    lamports,
                    rewards_recipient.key,
                ),
                &[
                    vote_account_info.clone(),
                    vault_info.clone(),
                    rewards_recipient.clone(),
                    vote_program_info.clone(),
                ],
                &[vault_account_signer_seeds],
            )
        }
    }
}

#[cfg(test)]
mod test {
    use {
        super::*,
        assert_matches::*,
        solana_program::{
            hash::Hash,
            instruction::AccountMeta,
            system_instruction,
            vote::{
                self,
                state::{VoteInit, VoteState},
            },
        },
        solana_program_test::*,
        solana_sdk::{
            signature::{Keypair, Signer},
            transaction::Transaction,
        },
        tokio::time::{sleep, Duration},
    };

    async fn create_vote_account(
        banks_client: &mut BanksClient,
        payer: &Keypair,
    ) -> (
        /*vote_account_keypair*/ Keypair,
        /*authorized_withdrawer_keypair*/ Keypair,
    ) {
        let node_pubkey_keypair = Keypair::new();
        let vote_account_keypair = Keypair::new();
        let authorized_withdrawer_keypair = Keypair::new();

        let rent = banks_client.get_rent().await.unwrap();

        let mut transaction = Transaction::new_with_payer(
            &vote::instruction::create_account(
                &payer.pubkey(),
                &vote_account_keypair.pubkey(),
                &VoteInit {
                    node_pubkey: node_pubkey_keypair.pubkey(),
                    authorized_voter: Pubkey::new_unique(),
                    authorized_withdrawer: authorized_withdrawer_keypair.pubkey(),
                    commission: 42,
                },
                rent.minimum_balance(VoteState::size_of()),
            ),
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[&payer, &vote_account_keypair, &node_pubkey_keypair],
            banks_client
                .get_latest_blockhash()
                .await
                .expect("blockhash"),
        );
        assert_matches!(banks_client.process_transaction(transaction).await, Ok(()));

        (vote_account_keypair, authorized_withdrawer_keypair)
    }

    async fn get_new_blockhash(banks_client: &mut BanksClient) -> Hash {
        let current_blockhash = banks_client
            .get_latest_blockhash()
            .await
            .expect("blockhash");
        loop {
            let new_blockhash = banks_client
                .get_latest_blockhash()
                .await
                .expect("blockhash");
            if new_blockhash != current_blockhash {
                return new_blockhash;
            }
            sleep(Duration::from_millis(00)).await;
        }
    }

    #[tokio::test]
    async fn test_enter() {
        let (mut banks_client, payer, _recent_blockhash) = ProgramTest::new(
            "sol_rewards_vault_program",
            crate::id(),
            processor!(process_instruction),
        )
        .start()
        .await;

        let (vote_account_keypair, authorized_withdrawer_keypair) =
            create_vote_account(&mut banks_client, &payer).await;

        let invalid_authorized_withdrawer_keypair = Keypair::new();
        let rewards_authority_keypair = Keypair::new();

        // invalid authorized withdrawer
        let mut transaction = Transaction::new_with_payer(
            &[crate::instruction::enter(
                vote_account_keypair.pubkey(),
                payer.pubkey(),
                invalid_authorized_withdrawer_keypair.pubkey(),
                rewards_authority_keypair.pubkey(),
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[&payer, &invalid_authorized_withdrawer_keypair],
            get_new_blockhash(&mut banks_client).await,
        );
        assert_matches!(banks_client.process_transaction(transaction).await, Err(_));

        // invalid vault address
        let mut instruction = crate::instruction::enter(
            vote_account_keypair.pubkey(),
            payer.pubkey(),
            authorized_withdrawer_keypair.pubkey(),
            rewards_authority_keypair.pubkey(),
        );
        instruction.accounts[0] = AccountMeta::new(
            crate::get_rewards_vault_address(&Pubkey::new_unique()),
            false,
        );
        let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
        transaction.sign(
            &[&payer, &authorized_withdrawer_keypair],
            get_new_blockhash(&mut banks_client).await,
        );
        assert_matches!(banks_client.process_transaction(transaction).await, Err(_));

        // enter ok
        let mut transaction = Transaction::new_with_payer(
            &[crate::instruction::enter(
                vote_account_keypair.pubkey(),
                payer.pubkey(),
                authorized_withdrawer_keypair.pubkey(),
                rewards_authority_keypair.pubkey(),
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[&payer, &authorized_withdrawer_keypair],
            get_new_blockhash(&mut banks_client).await,
        );
        assert_matches!(banks_client.process_transaction(transaction).await, Ok(()));

        // can't re-enter
        let mut transaction = Transaction::new_with_payer(
            &[crate::instruction::enter(
                vote_account_keypair.pubkey(),
                payer.pubkey(),
                authorized_withdrawer_keypair.pubkey(),
                rewards_authority_keypair.pubkey(),
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[&payer, &authorized_withdrawer_keypair],
            get_new_blockhash(&mut banks_client).await,
        );
        assert_matches!(banks_client.process_transaction(transaction).await, Err(_));
    }

    #[tokio::test]
    async fn test_leave() {
        let (mut banks_client, payer, _recent_blockhash) = ProgramTest::new(
            "sol_rewards_vault_program",
            crate::id(),
            processor!(process_instruction),
        )
        .start()
        .await;

        let (vote_account_keypair, authorized_withdrawer_keypair) =
            create_vote_account(&mut banks_client, &payer).await;

        let invalid_authorized_withdrawer_keypair = Keypair::new();
        let rewards_authority_keypair = Keypair::new();
        let refund_address = Pubkey::new_unique();

        // enter
        let mut transaction = Transaction::new_with_payer(
            &[crate::instruction::enter(
                vote_account_keypair.pubkey(),
                payer.pubkey(),
                authorized_withdrawer_keypair.pubkey(),
                rewards_authority_keypair.pubkey(),
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[&payer, &authorized_withdrawer_keypair],
            get_new_blockhash(&mut banks_client).await,
        );
        assert_matches!(banks_client.process_transaction(transaction).await, Ok(()));

        // leave: invalid authorized withdrawer
        let mut transaction = Transaction::new_with_payer(
            &[crate::instruction::leave(
                vote_account_keypair.pubkey(),
                payer.pubkey(),
                invalid_authorized_withdrawer_keypair.pubkey(),
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[&payer, &invalid_authorized_withdrawer_keypair],
            get_new_blockhash(&mut banks_client).await,
        );
        assert_matches!(banks_client.process_transaction(transaction).await, Err(_));

        // leave ok
        let mut transaction = Transaction::new_with_payer(
            &[crate::instruction::leave(
                vote_account_keypair.pubkey(),
                refund_address,
                authorized_withdrawer_keypair.pubkey(),
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[&payer, &authorized_withdrawer_keypair],
            get_new_blockhash(&mut banks_client).await,
        );
        assert_matches!(banks_client.process_transaction(transaction).await, Ok(()));

        assert_eq!(
            banks_client
                .get_balance(crate::get_rewards_vault_address(
                    &vote_account_keypair.pubkey()
                ))
                .await
                .unwrap(),
            0
        );

        let rent = banks_client.get_rent().await.unwrap();
        assert_eq!(
            banks_client.get_balance(refund_address).await.unwrap(),
            rent.minimum_balance(RewardsVaultState::size_of())
        );
    }

    #[tokio::test]
    async fn test_withdraw() {
        let (mut banks_client, payer, _recent_blockhash) = ProgramTest::new(
            "sol_rewards_vault_program",
            crate::id(),
            processor!(process_instruction),
        )
        .start()
        .await;

        let (vote_account_keypair, authorized_withdrawer_keypair) =
            create_vote_account(&mut banks_client, &payer).await;

        let minimum_vote_account_balance = banks_client
            .get_balance(vote_account_keypair.pubkey())
            .await
            .unwrap();
        let rewards_authority_keypair = Keypair::new();
        let epoch_rewards = 12345678;
        let rewards_recipient_address = Pubkey::new_unique();

        // Enter the rewards vault
        let mut transaction = Transaction::new_with_payer(
            &[crate::instruction::enter(
                vote_account_keypair.pubkey(),
                payer.pubkey(),
                authorized_withdrawer_keypair.pubkey(),
                rewards_authority_keypair.pubkey(),
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[&payer, &authorized_withdrawer_keypair],
            get_new_blockhash(&mut banks_client).await,
        );
        assert_matches!(banks_client.process_transaction(transaction).await, Ok(()));

        // Simulate epoch rewards
        let mut transaction = Transaction::new_with_payer(
            &[system_instruction::transfer(
                &payer.pubkey(),
                &vote_account_keypair.pubkey(),
                epoch_rewards,
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[&payer], get_new_blockhash(&mut banks_client).await);
        assert_matches!(banks_client.process_transaction(transaction).await, Ok(()));
        assert_eq!(
            banks_client
                .get_balance(vote_account_keypair.pubkey())
                .await
                .unwrap(),
            minimum_vote_account_balance + epoch_rewards
        );

        // Verify withdraw authority cannot withdraw the rewards
        let mut transaction = Transaction::new_with_payer(
            &[vote::instruction::withdraw(
                &vote_account_keypair.pubkey(),
                &authorized_withdrawer_keypair.pubkey(),
                1,
                &payer.pubkey(),
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[&payer, &authorized_withdrawer_keypair],
            get_new_blockhash(&mut banks_client).await,
        );
        assert_matches!(banks_client.process_transaction(transaction).await, Err(_));

        // Reward authority can withdraw the rewards
        let mut transaction = Transaction::new_with_payer(
            &[crate::instruction::withdraw_rewards(
                vote_account_keypair.pubkey(),
                rewards_recipient_address,
                rewards_authority_keypair.pubkey(),
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[&payer, &rewards_authority_keypair],
            get_new_blockhash(&mut banks_client).await,
        );
        assert_matches!(banks_client.process_transaction(transaction).await, Ok(()));

        assert_eq!(
            banks_client
                .get_balance(rewards_recipient_address)
                .await
                .unwrap(),
            epoch_rewards
        );
        assert_eq!(
            banks_client
                .get_balance(vote_account_keypair.pubkey())
                .await
                .unwrap(),
            minimum_vote_account_balance
        );

        // Leave the rewards vault
        let mut transaction = Transaction::new_with_payer(
            &[crate::instruction::leave(
                vote_account_keypair.pubkey(),
                payer.pubkey(),
                authorized_withdrawer_keypair.pubkey(),
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[&payer, &authorized_withdrawer_keypair],
            get_new_blockhash(&mut banks_client).await,
        );
        assert_matches!(banks_client.process_transaction(transaction).await, Ok(()));

        // Simulate epoch rewards
        let mut transaction = Transaction::new_with_payer(
            &[system_instruction::transfer(
                &payer.pubkey(),
                &vote_account_keypair.pubkey(),
                epoch_rewards,
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[&payer], get_new_blockhash(&mut banks_client).await);
        assert_matches!(banks_client.process_transaction(transaction).await, Ok(()));
        assert_eq!(
            banks_client
                .get_balance(vote_account_keypair.pubkey())
                .await
                .unwrap(),
            minimum_vote_account_balance + epoch_rewards
        );

        // Verify withdraw authority now can withdraw the rewards
        let mut transaction = Transaction::new_with_payer(
            &[vote::instruction::withdraw(
                &vote_account_keypair.pubkey(),
                &authorized_withdrawer_keypair.pubkey(),
                1,
                &payer.pubkey(),
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[&payer, &authorized_withdrawer_keypair],
            get_new_blockhash(&mut banks_client).await,
        );
        assert_matches!(banks_client.process_transaction(transaction).await, Ok(()));
    }
}
