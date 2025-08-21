use std::{ io::{self, BufRead}, env, sync::Arc };
use rand::Rng;
use solana_sdk::{
    signer::{keypair::Keypair, Signer},
    instruction::Instruction,
    system_instruction::transfer,
    transaction::Transaction,
    message::Message,
};
use solana_client::nonblocking::rpc_client::RpcClient;

const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
// Консервативная оценка комиссии за простой перевод (обычно ~5_000 л.).
const ESTIMATED_FEE_LAMPORTS: u64 = 5_000;

fn parse_amount_input(input: &str) -> Result<(u64, u64), anyhow::Error> {
    let s = input.trim();
    if let Some((a, b)) = s.split_once('-') {
        let min = (a.trim().parse::<f64>()? * LAMPORTS_PER_SOL as f64).round() as u64;
        let max = (b.trim().parse::<f64>()? * LAMPORTS_PER_SOL as f64).round() as u64;
        if min == 0 && max == 0 { anyhow::bail!("Диапазон не может быть 0-0"); }
        if min > max { anyhow::bail!("Минимум больше максимума в диапазоне"); }
        Ok((min, max))
    } else {
        let v = (s.parse::<f64>()? * LAMPORTS_PER_SOL as f64).round() as u64;
        if v == 0 { anyhow::bail!("Сумма не может быть 0"); }
        Ok((v, v))
    }
}

async fn sign_and_send_with_retry(
    client: &RpcClient,
    signers: &[&Keypair],
    msg: &Message,
    max_retries: usize,
) -> Result<solana_sdk::signature::Signature, anyhow::Error> {
    let mut attempt = 0usize;
    loop {
        attempt += 1;

        let recent_blockhash = client.get_latest_blockhash().await?;
        let mut tx = Transaction::new_unsigned(msg.clone());
        tx.try_sign(signers, recent_blockhash)?;

        match client.send_and_confirm_transaction(&tx).await {
            Ok(sig) => return Ok(sig),
            Err(e) => {
                let es = e.to_string();
                let is_blockhash_err = es.contains("blockhash not found") || es.contains("Invalid blockhash");
                if is_blockhash_err && attempt < max_retries {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    continue;
                }
                return Err(anyhow::anyhow!(e));
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    dotenv::dotenv().ok();

    let (rpc_url, main_wallet, threads): (String, Arc<Keypair>, usize) = (
        env::var("HTTP_RPC_URL")?,
        Arc::new(Keypair::from_base58_string(&env::var("MAIN_WALLET")?)),
        env::var("THREADS")?.parse()?
    );

    let semaphore = Arc::new(tokio::sync::Semaphore::new(threads));

    let sub_accounts: Vec<Arc<Keypair>> = tokio::fs::read_to_string("./files/accounts.txt")
        .await?
        .replace('\r', "")
        .trim()
        .split('\n')
        .map(Keypair::from_base58_string)
        .map(Arc::new)
        .collect::<Vec<_>>();

    let client: Arc<RpcClient> = Arc::new(RpcClient::new(rpc_url));

    println!("Upload {} subaccounts\n", sub_accounts.len());
    println!("Menu\n1 - From main to subaccounts\n2 - From subaccounts to main (will withdraw all)");

    let mut buffer: String = String::new();
    io::stdin().lock().read_line(&mut buffer)?;
    let choice: u32 = buffer.trim().parse()?;

    match choice {
        1 => {
            println!("Enter amount or range (e.g. 0.01 or 0.01-0.014):");
            let mut buffer: String = String::new();
            io::stdin().lock().read_line(&mut buffer)?;
            let (min_lamports, max_lamports) = parse_amount_input(&buffer)?;

            // Случайные суммы на каждого получателя
            let mut rng = rand::thread_rng();
            let mut amounts: Vec<u64> = Vec::with_capacity(sub_accounts.len());
            for _ in 0..sub_accounts.len() {
                let a = if min_lamports == max_lamports {
                    min_lamports
                } else {
                    rng.gen_range(min_lamports..=max_lamports)
                };
                amounts.push(a.max(1));
            }

            // Проверка баланса с запасом на комиссии
            let main_wallet_balance: u64 = client.get_balance(&main_wallet.pubkey()).await?;
            let total_amount: u64 = amounts.iter().copied().sum();
            let total_fees: u64 = ESTIMATED_FEE_LAMPORTS.saturating_mul(sub_accounts.len() as u64);
            let needed = total_amount.saturating_add(total_fees);

            if needed > main_wallet_balance {
                println!(
                    "Недостаточно средств: нужно {} lamports ({:.9} SOL) включая комиссии ~{} ({:.9} SOL). Баланс: {} ({:.9} SOL).",
                    needed,
                    needed as f64 / LAMPORTS_PER_SOL as f64,
                    total_fees,
                    total_fees as f64 / LAMPORTS_PER_SOL as f64,
                    main_wallet_balance,
                    main_wallet_balance as f64 / LAMPORTS_PER_SOL as f64
                );
                return Ok(());
            }

            let main_wallet_clone = Arc::clone(&main_wallet);
            let client_clone = Arc::clone(&client);

            let handles = sub_accounts
                .into_iter()
                .zip(amounts.into_iter())
                .map(|(sub_account, amount_lamports)| {
                    let semaphore = Arc::clone(&semaphore);
                    let main_wallet = Arc::clone(&main_wallet_clone);
                    let client = Arc::clone(&client_clone);
                    tokio::spawn(async move {
                        let _permit = semaphore.acquire_owned().await;

                        let ix: Instruction = transfer(&main_wallet.pubkey(), &sub_account.pubkey(), amount_lamports);
                        let msg = Message::new(&[ix], Some(&main_wallet.pubkey()));

                        match sign_and_send_with_retry(&client, &[&*main_wallet], &msg, 5).await {
                            Ok(sig) => {
                                println!(
                                    "Sent {} lamports ({:.9} SOL) to {}. Tx: {}",
                                    amount_lamports,
                                    amount_lamports as f64 / LAMPORTS_PER_SOL as f64,
                                    sub_account.pubkey(),
                                    sig
                                );
                            }
                            Err(e) => eprintln!("Error sending to {}: {:?}", sub_account.pubkey(), e),
                        }

                        Ok::<(), anyhow::Error>(())
                    })
                });

            futures::future::join_all(handles).await;
        }
        2 => {
            let handles = sub_accounts.iter().cloned().map(|sub_account| {
                let semaphore = Arc::clone(&semaphore);
                let main_wallet = Arc::clone(&main_wallet);
                let client = Arc::clone(&client);
                tokio::spawn(async move {
                    let _permit = semaphore.acquire_owned().await;

                    let sub_acc_balance: u64 = client.get_balance(&sub_account.pubkey()).await?;
                    if sub_acc_balance == 0 {
                        println!("Account {} has no balance", sub_account.pubkey());
                        return Ok::<(), anyhow::Error>(());
                    }

                    let ix: Instruction = transfer(&sub_account.pubkey(), &main_wallet.pubkey(), sub_acc_balance);
                    let msg = Message::new(&[ix], Some(&sub_account.pubkey()));

                    match sign_and_send_with_retry(&client, &[&*sub_account], &msg, 5).await {
                        Ok(sig) => {
                            println!(
                                "Sent {} lamports ({:.9} SOL) from {} to main. Tx: {}",
                                sub_acc_balance,
                                sub_acc_balance as f64 / LAMPORTS_PER_SOL as f64,
                                sub_account.pubkey(),
                                sig
                            );
                        }
                        Err(e) => eprintln!("Error withdrawing from {}: {:?}", sub_account.pubkey(), e),
                    }

                    Ok(())
                })
            });

            futures::future::join_all(handles).await;
        }
        _ => println!("Invalid choice"),
    }

    Ok(())
}
