use anyhow::{Result, ensure};
use clap::{Parser, Subcommand};
use ipnet::IpNet;
use std::{net::IpAddr, path::PathBuf};
#[derive(Parser)]
#[command(
    version,
    about = "KagNode. Network — inspect, plan and review networking"
)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    Capabilities,
    Inspect,
    Plan {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Ipam {
        #[arg(long, default_value = "/srv/kagnode/netctl")]
        root: PathBuf,
        #[command(subcommand)]
        command: Ipam,
    },
}
#[derive(Subcommand)]
enum Ipam {
    AddPool {
        #[arg(long)]
        name: String,
        #[arg(long)]
        subnet: IpNet,
        #[arg(long)]
        start: IpAddr,
        #[arg(long)]
        end: IpAddr,
    },
    Allocate {
        #[arg(long)]
        pool: String,
        #[arg(long)]
        owner: String,
    },
    Release {
        #[arg(long)]
        pool: String,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        address: IpAddr,
    },
    List,
}
fn main() -> Result<()> {
    let args = Args::parse();
    let result = match args.command {
        Commands::Capabilities => kagnode_netctl::capability(),
        Commands::Inspect => kagnode_netctl::observe::host()?,
        Commands::Plan { file, output } => {
            ensure!(
                std::fs::metadata(&file)?.len() <= 1024 * 1024,
                "Network declaration exceeds one MiB"
            );
            let config: kagnode_netctl::Network = serde_json::from_slice(&std::fs::read(file)?)?;
            let plan = config.plan()?;
            if let Some(output) = output {
                plan.write_new(&output)?;
            }
            serde_json::to_value(plan)?
        }
        Commands::Ipam { root, command } => {
            let mut ipam = kagnode_netctl::ipam::Ipam::open(&root)?;
            match command {
                Ipam::AddPool {
                    name,
                    subnet,
                    start,
                    end,
                } => {
                    ipam.add_pool(&name, subnet, start, end)?;
                    serde_json::json!({"created":name})
                }
                Ipam::Allocate { pool, owner } => {
                    serde_json::to_value(ipam.allocate(&pool, &owner)?)?
                }
                Ipam::Release {
                    pool,
                    owner,
                    address,
                } => serde_json::json!({"released":ipam.release(&pool,&owner,address)?}),
                Ipam::List => serde_json::to_value(ipam.leases()?)?,
            }
        }
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
