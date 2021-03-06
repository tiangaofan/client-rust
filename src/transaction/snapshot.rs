// Copyright 2019 TiKV Project Authors. Licensed under Apache-2.0.

use crate::Transaction;
use derive_new::new;
use futures::stream::BoxStream;
use std::ops::RangeBounds;
use tikv_client_common::{BoundRange, Key, KvPair, Result, Value};

/// A readonly transaction which can have a custom timestamp.
///
/// See the [Transaction](transaction) docs for more information on the methods.
#[derive(new)]
pub struct Snapshot {
    transaction: Transaction,
}

impl Snapshot {
    /// Gets the value associated with the given key.
    pub async fn get(&self, key: impl Into<Key>) -> Result<Option<Value>> {
        self.transaction.get(key).await
    }

    /// Gets the values associated with the given keys.
    pub async fn batch_get(
        &self,
        keys: impl IntoIterator<Item = impl Into<Key>>,
    ) -> Result<impl Iterator<Item = KvPair>> {
        self.transaction.batch_get(keys).await
    }

    pub async fn scan(
        &self,
        range: impl Into<BoundRange>,
        limit: u32,
    ) -> Result<impl Iterator<Item = KvPair>> {
        self.transaction.scan(range, limit).await
    }

    pub fn scan_reverse(&self, range: impl RangeBounds<Key>) -> BoxStream<Result<KvPair>> {
        self.transaction.scan_reverse(range)
    }
}
