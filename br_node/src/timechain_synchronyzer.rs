use brl::{hashing::hash256::Hash256, std_lib::std_result::StdResult};
use tokio::sync::broadcast::{Receiver, Sender};

use crate::{
    database::{postgres_repository::PostgresRepository, repository::Repository},
    node_message::NodeMessage,
};

pub async fn start(
    start_block_hash: Hash256,
    sender: Sender<NodeMessage>,
    mut receiver: Receiver<NodeMessage>,
) -> StdResult<()> {
    // Connecting to database
    let mut repo = PostgresRepository::connect()?;

    loop {
        // get last block in persistence and use its hash as starting block
        // for now use genesis_block_hash as starting block
        let message = receiver.recv().await?;

        match message {
            NodeMessage::NodeReady(node_id) => {
                log::debug!("Node {} is ready", node_id);
                let _ = sender.send(NodeMessage::GetHeadersRequest(node_id, start_block_hash));
            }
            NodeMessage::HeadersResponse(node_id, headers) => {
                log::debug!(
                    "Received {:?} headers from NID-{}: writing into database",
                    node_id,
                    headers.0.len()
                );
                repo.create_headers(&headers.0).await?;

                // let _ = sender.send(NodeMessage::GetHeadersRequest(start_block_hash));
            }
            _ => {
                log::info!("Received message: {:?}", message);
                log::debug!("timechain_synchronyzer exiting...");

                break;
            }
        }
    }

    Ok(())
}