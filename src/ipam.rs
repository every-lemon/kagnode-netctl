use anyhow::{Result, ensure};
use ipnet::IpNet;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
};
pub struct Ipam {
    db: Connection,
}
#[derive(Serialize)]
pub struct Lease {
    pub pool: String,
    pub owner: String,
    pub address: IpAddr,
}
fn integer(ip: IpAddr) -> u128 {
    match ip {
        IpAddr::V4(v) => u32::from(v) as u128,
        IpAddr::V6(v) => u128::from(v),
    }
}
fn address(value: u128, v4: bool) -> IpAddr {
    if v4 {
        IpAddr::V4(Ipv4Addr::from(value as u32))
    } else {
        IpAddr::V6(Ipv6Addr::from(value))
    }
}
impl Ipam {
    pub fn open(root: &Path) -> Result<Self> {
        ensure!(root.is_absolute(), "Use an absolute private IPAM directory");
        for parent in root.ancestors().filter(|p| p.exists()) {
            ensure!(!parent.is_symlink(), "IPAM directories cannot be symlinks");
        }
        std::fs::create_dir_all(root)?;
        ensure!(
            root.metadata()?.uid() == unsafe { libc::geteuid() },
            "IPAM directory has another owner"
        );
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
        let file = root.join("ipam.sqlite");
        ensure!(!file.is_symlink(), "Unsafe IPAM database");
        if file.exists() {
            let m = file.symlink_metadata()?;
            ensure!(
                m.is_file() && m.nlink() == 1 && m.uid() == unsafe { libc::geteuid() },
                "Unsafe IPAM database ownership"
            );
        }
        let db = Connection::open(&file)?;
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600))?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS pools(name TEXT PRIMARY KEY,subnet TEXT NOT NULL,start TEXT NOT NULL,end TEXT NOT NULL);CREATE TABLE IF NOT EXISTS leases(pool TEXT NOT NULL REFERENCES pools(name),owner TEXT NOT NULL,address TEXT NOT NULL UNIQUE,PRIMARY KEY(pool,owner));")?;
        Ok(Self { db })
    }
    pub fn add_pool(
        &mut self,
        name: &str,
        subnet: IpNet,
        start: IpAddr,
        end: IpAddr,
    ) -> Result<()> {
        ensure!(crate::name(name), "Invalid pool name");
        ensure!(
            subnet.contains(&start) && subnet.contains(&end) && start.is_ipv4() == end.is_ipv4(),
            "IPAM pool addresses must belong to its subnet"
        );
        ensure!(
            integer(start) <= integer(end) && integer(end) - integer(start) < 65536,
            "Choose an ordered IPAM allocation window of at most 65536 addresses"
        );
        ensure!(
            !start.is_unspecified() && !start.is_multicast() && !end.is_multicast(),
            "Choose unicast allocation addresses"
        );
        let tx = self.db.transaction()?;
        {
            let mut q = tx.prepare("SELECT start,end FROM pools")?;
            for row in q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
                let (a, b) = row?;
                let (a, b) = (a.parse::<IpAddr>()?, b.parse::<IpAddr>()?);
                ensure!(
                    a.is_ipv4() != start.is_ipv4()
                        || integer(start) > integer(b)
                        || integer(end) < integer(a),
                    "IPAM allocation windows overlap"
                );
            }
        }
        tx.execute(
            "INSERT INTO pools VALUES(?1,?2,?3,?4)",
            params![name, subnet.to_string(), start.to_string(), end.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn allocate(&mut self, pool: &str, owner: &str) -> Result<Lease> {
        ensure!(
            !owner.is_empty() && owner.len() <= 200 && !owner.chars().any(char::is_control),
            "Invalid lease owner"
        );
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(raw) = tx
            .query_row(
                "SELECT address FROM leases WHERE pool=?1 AND owner=?2",
                params![pool, owner],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(Lease {
                pool: pool.into(),
                owner: owner.into(),
                address: raw.parse()?,
            });
        }
        let (start, end) =
            tx.query_row("SELECT start,end FROM pools WHERE name=?1", [pool], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
        let (start, end) = (start.parse::<IpAddr>()?, end.parse::<IpAddr>()?);
        let mut chosen = None;
        for v in integer(start)..=integer(end) {
            let a = address(v, start.is_ipv4());
            let exists = tx
                .query_row(
                    "SELECT 1 FROM leases WHERE address=?1",
                    [a.to_string()],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !exists {
                chosen = Some(a);
                break;
            }
        }
        let chosen =
            chosen.ok_or_else(|| anyhow::anyhow!("IPAM allocation window is exhausted"))?;
        tx.execute(
            "INSERT INTO leases VALUES(?1,?2,?3)",
            params![pool, owner, chosen.to_string()],
        )?;
        tx.commit()?;
        Ok(Lease {
            pool: pool.into(),
            owner: owner.into(),
            address: chosen,
        })
    }
    pub fn release(&mut self, pool: &str, owner: &str, address: IpAddr) -> Result<bool> {
        Ok(self.db.execute(
            "DELETE FROM leases WHERE pool=?1 AND owner=?2 AND address=?3",
            params![pool, owner, address.to_string()],
        )? == 1)
    }
    pub fn leases(&self) -> Result<Vec<Lease>> {
        let mut q = self
            .db
            .prepare("SELECT pool,owner,address FROM leases ORDER BY pool,owner")?;
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(pool, owner, address)| {
                Ok(Lease {
                    pool,
                    owner,
                    address: address.parse()?,
                })
            })
            .collect()
    }
}
