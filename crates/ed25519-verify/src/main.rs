use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use ed25519_dalek::{Verifier, Signature, VerifyingKey, SigningKey, Signer};
use std::io::Read;

#[derive(Parser, Debug)]
#[command(name="ed25519-verify", about="Ed25519 helper (verify | derive-pubkey | sign)")]
struct Cli {
    #[command(subcommand)] cmd: Cmd
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Verify signature: reads msg from stdin, needs env AETHER_PUBKEY
    Verify { signature_hex: String },
    /// Derive public key from 32-byte seed hex
    Pubkey { seed_hex: String },
    /// Sign message from stdin with seed hex
    Sign { seed_hex: String },
}

fn main() -> Result<()> { if let Err(e)=real_main(){ eprintln!("{e}"); std::process::exit(1); } Ok(()) }

fn real_main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Verify { signature_hex } => do_verify(&signature_hex),
        Cmd::Pubkey { seed_hex } => { let (pk,_) = derive_keys(&seed_hex)?; println!("{pk}"); Ok(()) },
        Cmd::Sign { seed_hex } => { let (_,sk) = derive_keys(&seed_hex)?; let mut msg=Vec::new(); std::io::stdin().read_to_end(&mut msg)?; let sig = sk.sign(&msg); println!("{}", hex::encode(sig.to_bytes())); Ok(()) }
    }
}

fn derive_keys(seed_hex:&str) -> Result<(String, SigningKey)> {
    let seed = hex::decode(seed_hex)?; if seed.len()!=32 { return Err(anyhow!("seed must be 32 bytes")); }
    let mut seed_arr=[0u8;32]; seed_arr.copy_from_slice(&seed);
    let sk = SigningKey::from_bytes(&seed_arr);
    let pk = sk.verifying_key();
    Ok((hex::encode(pk.as_bytes()), sk))
}

fn do_verify(signature_hex:&str) -> Result<()> {
    let sig_bytes = hex::decode(signature_hex)?;
    let sig = Signature::from_slice(&sig_bytes).map_err(|_| anyhow!("invalid signature length"))?;
    let pk_hex = std::env::var("AETHER_PUBKEY").map_err(|_| anyhow!("AETHER_PUBKEY env missing"))?;
    let pk_bytes = hex::decode(pk_hex)?; if pk_bytes.len()!=32 { return Err(anyhow!("invalid public key length")); }
    let mut pk_arr=[0u8;32]; pk_arr.copy_from_slice(&pk_bytes);
    let vk = VerifyingKey::from_bytes(&pk_arr).map_err(|_| anyhow!("invalid public key"))?;
    let mut msg=Vec::new(); std::io::stdin().read_to_end(&mut msg)?;
    vk.verify(&msg, &sig).map_err(|_| anyhow!("verification failed"))?; Ok(())
}
