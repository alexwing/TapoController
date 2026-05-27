//! M0 spike CLI: prove cloud-free control of the real bulb.
//!
//! Examples (host defaults to the user's bulb 192.168.1.226):
//!   tapo-cli -u <user> -p <pass> info
//!   tapo-cli -u <user> -p <pass> power --on
//!   tapo-cli -u <user> -p <pass> power --off
//!   tapo-cli -u <user> -p <pass> brightness 60
//!   tapo-cli -u <user> -p <pass> color --hue 120 --sat 100
//!   tapo-cli -u <user> -p <pass> rgb 255 80 0
//!   tapo-cli -u <user> -p <pass> temp 4000
//!
//! Credentials can also come from env: TAPO_USER / TAPO_PASS.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tapo_proto::TapoDevice;

#[derive(Parser)]
#[command(name = "tapo-cli", about = "Cloud-free Tapo KLAP spike client")]
struct Cli {
    /// Device IP or hostname (no scheme).
    #[arg(long, default_value = "192.168.1.226")]
    host: String,

    /// Local device username (not needed for `doctor`).
    #[arg(short = 'u', long, env = "TAPO_USER")]
    user: Option<String>,

    /// Local device password (not needed for `doctor`).
    #[arg(short = 'p', long, env = "TAPO_PASS")]
    pass: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Diagnose connectivity/firmware gate (no credentials required).
    Doctor,
    /// Scan the local network for Tapo devices (no credentials required).
    Discover,
    /// Read and print device info (raw JSON).
    Info,
    /// Turn the bulb on or off.
    Power {
        #[arg(long, conflicts_with = "off")]
        on: bool,
        #[arg(long)]
        off: bool,
    },
    /// Set brightness 1..=100.
    Brightness { value: u8 },
    /// Set HSV color.
    Color {
        #[arg(long)]
        hue: u16,
        #[arg(long = "sat")]
        saturation: u8,
    },
    /// Set sRGB color (0..=255 each).
    Rgb { r: u8, g: u8, b: u8 },
    /// Set color temperature in Kelvin (2500..=6500).
    Temp { kelvin: u16 },
    /// Call any method directly. `params` is optional JSON.
    Raw {
        method: String,
        #[arg(default_value = "")]
        params: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();

    if let Cmd::Discover = cli.cmd {
        eprintln!("[info] scanning local /24…");
        let found = tapo_proto::discover().await;
        if found.is_empty() {
            println!("(no Tapo devices found)");
        }
        for d in found {
            println!("{}  klap={}", d.ip, d.klap);
        }
        return Ok(());
    }

    // `doctor` needs no credentials and no session.
    if let Cmd::Doctor = cli.cmd {
        let d = tapo_proto::diagnose(&cli.host).await;
        println!("Host:        {}", d.host);
        println!("Reachable:   {}", d.reachable);
        println!("handshake1:  {:?}", d.handshake1_status);
        println!("/ status:    {:?}", d.app_root_status);
        println!("Verdict:     {:?}", d.verdict);
        println!("\n{}", d.message);
        return Ok(());
    }

    let (user, pass) = match (cli.user.clone(), cli.pass.clone()) {
        (Some(u), Some(p)) => (u, p),
        _ => anyhow::bail!("this command needs -u/--user and -p/--pass (or TAPO_USER/TAPO_PASS)"),
    };

    let proto = tapo_proto::detect_protocol(&cli.host)
        .await
        .with_context(|| format!("probing protocol on {}", cli.host))?;
    eprintln!("[info] detected protocol: {proto:?}");

    let dev = TapoDevice::connect(&cli.host, &user, &pass)
        .await
        .with_context(|| format!("connecting/handshaking with {}", cli.host))?;
    eprintln!("[ok] session established ({:?})", dev.protocol());

    match cli.cmd {
        Cmd::Doctor | Cmd::Discover => unreachable!("handled before session setup"),
        Cmd::Info => {
            let raw = dev.get_device_info_raw().await?;
            println!("{}", serde_json::to_string_pretty(&raw)?);
        }
        Cmd::Power { on, off } => {
            let state = on || !off;
            dev.set_power(state).await?;
            eprintln!("[ok] power -> {}", if state { "ON" } else { "OFF" });
        }
        Cmd::Brightness { value } => {
            dev.set_brightness(value).await?;
            eprintln!("[ok] brightness -> {value}");
        }
        Cmd::Color { hue, saturation } => {
            dev.set_hue_saturation(hue, saturation).await?;
            eprintln!("[ok] color -> hue={hue} sat={saturation}");
        }
        Cmd::Rgb { r, g, b } => {
            dev.set_rgb(r, g, b).await?;
            eprintln!("[ok] rgb -> ({r},{g},{b})");
        }
        Cmd::Temp { kelvin } => {
            dev.set_color_temp(kelvin).await?;
            eprintln!("[ok] color_temp -> {kelvin}K");
        }
        Cmd::Raw { method, params } => {
            let p = if params.trim().is_empty() {
                None
            } else {
                Some(serde_json::from_str(&params)?)
            };
            let v = dev.call_raw(&method, p).await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
    }
    Ok(())
}
