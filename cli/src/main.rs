use {
    clap::{crate_description, crate_name, crate_version, Arg, Command},
    solana_clap_v3_utils::{
        input_parsers::{pubkey_of, signer_of},
        input_validators::{
            is_url_or_moniker, is_valid_pubkey, is_valid_signer, normalize_to_url_if_moniker,
        },
        keypair::DefaultSigner,
    },
    solana_client::nonblocking::rpc_client::RpcClient,
    solana_remote_wallet::remote_wallet::RemoteWalletManager,
    solana_sdk::{
        commitment_config::CommitmentConfig, message::Message,
        signers::Signers, transaction::Transaction,
    },
    std::{process::exit, sync::Arc},
};

async fn send_message<T: Signers>(
    rpc_client: &RpcClient,
    message: Message,
    signers: &T,
) -> Result<(), String> {
    let mut transaction = Transaction::new_unsigned(message);

    let blockhash = rpc_client
        .get_latest_blockhash()
        .await
        .map_err(|err| format!("error: unable to get latest blockhash: {}", err))?;

    transaction
        .try_sign(signers, blockhash)
        .map_err(|err| format!("error: failed to sign transaction: {}", err))?;

    let signature = rpc_client
        .send_and_confirm_transaction_with_spinner(&transaction)
        .await
        .map_err(|err| format!("error: send transaction: {}", err))?;

    println!("Success: {}", signature);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app_matches = Command::new(crate_name!())
        .about(crate_description!())
        .version(crate_version!())
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg({
            let arg = Arg::new("config_file")
                .short('C')
                .long("config")
                .value_name("PATH")
                .takes_value(true)
                .global(true)
                .help("Configuration file to use");
            if let Some(ref config_file) = *solana_cli_config::CONFIG_FILE {
                arg.default_value(config_file)
            } else {
                arg
            }
        })
        .arg(
            Arg::new("fee_payer")
                .long("fee_payer")
                .value_name("KEYPAIR")
                .validator(|s| is_valid_signer(s))
                .takes_value(true)
                .global(true)
                .help("Specify the fee-payer account"),
        )
        .arg(
            Arg::new("verbose")
                .long("verbose")
                .short('v')
                .takes_value(false)
                .global(true)
                .help("Show additional information"),
        )
        .arg(
            Arg::new("json_rpc_url")
                .short('u')
                .long("url")
                .value_name("URL")
                .takes_value(true)
                .global(true)
                .validator(|s| is_url_or_moniker(s))
                .help("JSON RPC URL for the cluster [default: value from configuration file]"),
        )
        .subcommand(
            Command::new("enter")
                .about("Place a vote account in its rewards vault")
                .arg(
                    Arg::new("vote_account")
                        .validator(|s| is_valid_pubkey(s))
                        .value_name("VOTE_ACCOUNT_ADDRESS")
                        .takes_value(true)
                        .required(true)
                        .help("Vote account address"),
                )
                .arg(
                    Arg::new("withdraw_authority")
                        .value_name("WITHDRAW_AUTHORITY_KEYPAIR")
                        .validator(|s| is_valid_signer(s))
                        .takes_value(true)
                        .required(true)
                        .help("Vote account withdraw authority"),
                )
                .arg(
                    Arg::new("rewards_authority")
                        .validator(|s| is_valid_pubkey(s))
                        .value_name("REWARDS_AUTHORITY_ADDRESS")
                        .takes_value(true)
                        .required(true)
                        .help("Authority to withdraw rewards while vote account resides in its rewards vault"),
                )
        )
        .subcommand(
            Command::new("leave")
                .about("Remove a vote account from its rewards vault")
                .arg(
                    Arg::new("vote_account")
                        .validator(|s| is_valid_pubkey(s))
                        .value_name("VOTE_ACCOUNT_ADDRESS")
                        .takes_value(true)
                        .required(true)
                        .help("Vote account address"),
                )
                .arg(
                    Arg::new("withdraw_authority")
                        .value_name("WITHDRAW_AUTHORITY_KEYPAIR")
                        .validator(|s| is_valid_signer(s))
                        .takes_value(true)
                        .required(true)
                        .help("Vote account withdraw authority"),
                )
        )
        .subcommand(
            Command::new("withdraw")
                .about("Claim epoch rewards earned by a vote account residing in its rewards vault")
                .arg(
                    Arg::new("vote_account")
                        .validator(|s| is_valid_pubkey(s))
                        .value_name("VOTE_ACCOUNT_ADDRESS")
                        .takes_value(true)
                        .required(true)
                        .help("Vote account address"),
                )
                .arg(
                    Arg::new("rewards_authority")
                        .value_name("REWARDS_AUTHORITY_KEYPAIR")
                        .validator(|s| is_valid_signer(s))
                        .takes_value(true)
                        .required(true)
                        .help("Rewards authority"),
                )
                .arg(
                    Arg::new("rewards_recipient")
                        .validator(|s| is_valid_pubkey(s))
                        .value_name("REWARDS_RECIPIENT_ADDRESS")
                        .takes_value(true)
                        .help("Account to credit the epoch rewards to [default: Rewards authority]"),
                )

        )
        .get_matches();

    let (command, matches) = app_matches.subcommand().unwrap();
    let mut wallet_manager: Option<Arc<RemoteWalletManager>> = None;

    let cli_config = if let Some(config_file) = matches.value_of("config_file") {
        solana_cli_config::Config::load(config_file).unwrap_or_default()
    } else {
        solana_cli_config::Config::default()
    };

    let fee_payer = DefaultSigner::new(
        "fee_payer",
        matches
            .value_of(&"fee_payer")
            .map(|s| s.to_string())
            .unwrap_or_else(|| cli_config.keypair_path.clone()),
    );

    let json_rpc_url = normalize_to_url_if_moniker(
        matches
            .value_of("json_rpc_url")
            .unwrap_or(&cli_config.json_rpc_url),
    );

    let fee_payer = fee_payer
        .signer_from_path(matches, &mut wallet_manager)
        .unwrap_or_else(|err| {
            eprintln!("error: {}", err);
            exit(1);
        });
    let verbose = matches.is_present("verbose");

    solana_logger::setup_with_default("solana=info");

    if verbose {
        println!("JSON RPC URL: {}", json_rpc_url);
    }
    let rpc_client = RpcClient::new_with_commitment(json_rpc_url, CommitmentConfig::confirmed());

    match (command, matches) {
        ("enter", arg_matches) => {
            let vote_account = pubkey_of(arg_matches, "vote_account").unwrap();
            let (withdraw_authority_signer, withdraw_authority) = {
                let (withdraw_authority_signer, withdraw_authority) = signer_of(
                    arg_matches,
                    "withdraw_authority",
                    &mut wallet_manager,
                )?;
                (
                    withdraw_authority_signer.expect("withdraw_authority_signer"),
                    withdraw_authority.expect("withdraw_authority"),
                )
            };
            let rewards_authority = pubkey_of(arg_matches, "rewards_authority").unwrap();

            send_message(
                &rpc_client,
                Message::new(
                    &[sol_rewards_vault_program::instruction::enter(
                        vote_account,
                        fee_payer.pubkey(),
                        withdraw_authority,
                        rewards_authority,
                    )],
                    Some(&fee_payer.pubkey()),
                ),
                &vec![fee_payer, withdraw_authority_signer],
            )
            .await?;
        }
        ("leave", arg_matches) => {
            let vote_account = pubkey_of(arg_matches, "vote_account").unwrap();
            let (withdraw_authority_signer, withdraw_authority) = {
                let (withdraw_authority_signer, withdraw_authority) = signer_of(
                    arg_matches,
                    "withdraw_authority",
                    &mut wallet_manager,
                )?;
                (
                    withdraw_authority_signer.expect("withdraw_authority_signer"),
                    withdraw_authority.expect("withdraw_authority"),
                )
            };

            send_message(
                &rpc_client,
                Message::new(
                    &[sol_rewards_vault_program::instruction::leave(
                        vote_account,
                        fee_payer.pubkey(),
                        withdraw_authority,
                    )],
                    Some(&fee_payer.pubkey()),
                ),
                &vec![fee_payer, withdraw_authority_signer],
            )
            .await?;
        }
        ("withdraw", arg_matches) => {
            let vote_account = pubkey_of(arg_matches, "vote_account").unwrap();
            let (rewards_authority_signer, rewards_authority) = {
                let (rewards_authority_signer, rewards_authority) = signer_of(
                    arg_matches,
                    "rewards_authority",
                    &mut wallet_manager,
                )?;
                (
                    rewards_authority_signer.expect("rewards_authority_signer"),
                    rewards_authority.expect("rewards_authority"),
                )
            };
            let rewards_recipient =
                pubkey_of(arg_matches, "rewards_recipient").unwrap_or(rewards_authority);

            send_message(
                &rpc_client,
                Message::new(
                    &[sol_rewards_vault_program::instruction::withdraw_rewards(
                        vote_account,
                        rewards_recipient,
                        rewards_authority,
                    )],
                    Some(&fee_payer.pubkey()),
                ),
                &vec![fee_payer, rewards_authority_signer],
            )
            .await?;
        }
        _ => unreachable!(),
    };

    Ok(())
}
