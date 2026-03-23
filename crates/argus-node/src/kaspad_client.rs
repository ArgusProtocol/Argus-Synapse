use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info, warn, debug};
use serde::{Deserialize, Serialize};

use argus_ghostdag::{DagStore, BlockHash, BlockHeader};
use argus_agent::channels::CommandTx;
use argus_agent::channels::AgentCommand;

#[derive(Debug, Serialize, Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct BlockDagInfo {
    tip_hashes: Vec<String>,
    block_count: u64,
}

#[derive(Debug, Deserialize)]
struct KaspadBlock {
    header: KaspadHeader,
}

#[derive(Debug, Deserialize)]
struct KaspadHeader {
    hash: String,
    parent_hashes: Vec<String>,
    timestamp: u64,
    blue_score: u64,
}

pub struct KaspadClient {
    rpc_url: String,
    dag: Arc<RwLock<DagStore>>,
    cmd_tx: CommandTx,
    http_client: reqwest::Client,
}

#[async_trait::async_trait]
impl argus_agent::BlockFetcher for KaspadClient {
    async fn fetch_block(&self, hash: &BlockHash) -> anyhow::Result<BlockHeader> {
        self.get_block(hash).await
    }
}

impl KaspadClient {
    pub fn new(rpc_url: String, dag: Arc<RwLock<DagStore>>, cmd_tx: CommandTx) -> Self {
        Self {
            rpc_url,
            dag,
            cmd_tx,
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn get_block(&self, hash: &BlockHash) -> anyhow::Result<BlockHeader> {
        let req = RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "getBlock".to_string(),
            params: serde_json::json!({ "hash": hash.to_hex(), "includeRawBlockData": false }),
        };

        let resp: RpcResponse<KaspadBlock> = self.http_client
            .post(&self.rpc_url)
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        if let Some(res) = resp.result {
            let h = res.header;
            let mut parents = Vec::new();
            for p_hex in h.parent_hashes {
                parents.push(BlockHash::from_hex(&p_hex)?);
            }
            let mut header = BlockHeader::new(BlockHash::from_hex(&h.hash)?, parents, h.timestamp);
            header.blue_score = h.blue_score;
            Ok(header)
        } else {
            anyhow::bail!("Block not found: {}", hash.to_hex());
        }
    }

    pub async fn get_dag_info(&self) -> anyhow::Result<BlockDagInfo> {
        let req = RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "getBlockDagInfo".to_string(),
            params: serde_json::json!({}),
        };

        let resp: RpcResponse<BlockDagInfo> = self.http_client
            .post(&self.rpc_url)
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        if let Some(res) = resp.result {
            Ok(res)
        } else {
            anyhow::bail!("Failed to get DAG info");
        }
    }

    pub async fn run_ingestion_loop(self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        info!("Kaspad ingestion loop started");
        let mut interval = tokio::time::interval(Duration::from_millis(500));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.ingest_new_blocks().await {
                        error!("Ingestion error: {e}");
                    }
                }
                Ok(()) = shutdown.changed() => {
                    if *shutdown.borrow() {
                        info!("Kaspad ingestion loop shutting down");
                        break;
                    }
                }
            }
        }
    }

    async fn ingest_new_blocks(&self) -> anyhow::Result<()> {
        let info = self.get_dag_info().await?;
        
        // In a real implementation, we would use a WebSocket subscription.
        // For this MVP, we poll the tips and fetch any missing blocks.
        for tip_hex in info.tip_hashes {
            let tip_hash = BlockHash::from_hex(&tip_hex)?;
            
            // If we don't have this tip, we need to fetch it (and its parents).
            if !self.dag.read().await.contains(&tip_hash) {
                debug!("New tip detected: {}", tip_hex);
                self.fetch_and_ingest_recursively(tip_hash).await?;
                
                // Signal the agent that there's a new network tip.
                let _ = self.cmd_tx.send(AgentCommand::UpdateNetworkTip { tip: tip_hash }).await;
            }
        }
        
        Ok(())
    }

    #[async_recursion::async_recursion]
    async fn fetch_and_ingest_recursively(&self, hash: BlockHash) -> anyhow::Result<()> {
        if self.dag.read().await.contains(&hash) {
            return Ok(());
        }

        let header = self.get_block(&hash).await?;
        
        // Fetch parents first to maintain DAG invariants.
        for parent in &header.parents {
            self.fetch_and_ingest_recursively(*parent).await?;
        }

        // Add to local DAG.
        let mut dag = self.dag.write().await;
        if !dag.contains(&hash) {
            dag.add_block(header)?;
        }
        
        Ok(())
    }
}
