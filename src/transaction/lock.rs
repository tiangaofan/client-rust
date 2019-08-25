use super::requests;
use crate::pd::{PdClient, RegionVerId};
use crate::request::KvRequest;
use crate::{ErrorKind, Key, Result, Timestamp};

use kvproto::kvrpcpb;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub async fn resolve_locks(
    locks: Vec<kvrpcpb::LockInfo>,
    pd_client: Arc<impl PdClient>,
) -> Result<()> {
    let ts = pd_client.clone().get_timestamp().await?;
    let expired_locks = locks.into_iter().filter(|lock| {
        ts.physical - Timestamp::from_version(lock.lock_version).physical >= lock.lock_ttl as i64
    });

    // records the commit version of each primary lock (representing the status of the transaction)
    let mut commit_versions: HashMap<u64, u64> = HashMap::new();
    let mut clean_regions: HashMap<u64, HashSet<RegionVerId>> = HashMap::new();
    for lock in expired_locks {
        let primary_key: Key = lock.primary_lock.into();
        let region_ver_id = pd_client.region_for_key(&primary_key).await?.ver_id();
        // skip if the region is cleaned
        if clean_regions
            .get(&lock.lock_version)
            .map(|regions| regions.contains(&region_ver_id))
            .unwrap_or(false)
        {
            continue;
        }

        let commit_version = match commit_versions.get(&lock.lock_version) {
            Some(&commit_version) => commit_version,
            None => {
                let commit_version = requests::new_cleanup_request(primary_key, lock.lock_version)
                    .execute(pd_client.clone())
                    .await?;
                commit_versions.insert(lock.lock_version, commit_version);
                commit_version
            }
        };

        let cleaned_region = resolve_lock_with_retry(
            lock.key.into(),
            lock.lock_version,
            commit_version,
            pd_client.clone(),
        )
        .await?;
        clean_regions
            .entry(lock.lock_version)
            .or_insert_with(HashSet::new)
            .insert(cleaned_region);
    }
    Ok(())
}

async fn resolve_lock_with_retry(
    key: Key,
    start_version: u64,
    commit_version: u64,
    pd_client: Arc<impl PdClient>,
) -> Result<RegionVerId> {
    // TODO: Add backoff and retry limit
    loop {
        let region = pd_client.region_for_key(&key).await?;
        let context = match region.context() {
            Ok(context) => context,
            Err(_) => {
                // Retry if the region has no leader
                continue;
            }
        };
        match requests::new_resolve_lock_request(context, start_version, commit_version)
            .execute(pd_client.clone())
            .await
        {
            Ok(_) => {
                return Ok(region.ver_id());
            }
            Err(e) => match e.kind() {
                ErrorKind::RegionError(_) => {
                    // Retry on region error
                    continue;
                }
                _ => return Err(e),
            },
        }
    }
}

pub trait HasLocks {
    fn take_locks(&mut self) -> Vec<kvrpcpb::LockInfo>;
}

macro_rules! dummy_impl_has_locks {
    ($t: tt) => {
        impl crate::transaction::HasLocks for kvrpcpb::$t {
            fn take_locks(&mut self) -> Vec<kvrpcpb::LockInfo> {
                Vec::new()
            }
        }
    };
}
