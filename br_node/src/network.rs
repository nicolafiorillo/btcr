use brl::{flags::network_magic::NetworkMagic, std_lib::std_result::StdResult};
use tokio::net::TcpStream;

use crate::connection::Connection;

pub async fn connect_to_node(address: &str, network: NetworkMagic) -> StdResult<()> {
    log::info!("Connecting to remote node: {}", address);

    let stream = connect(address, network).await?;

    let mut connection = Connection::new(stream, network).await?;

    connection.try_handshake().await?;
    log::info!("Connection established.");

    connection.listen().await?;

    connection.shutdown().await?;

    Ok(())
}

async fn connect(address: &str, network: NetworkMagic) -> StdResult<TcpStream> {
    log::info!("Connecting to {} using {:?} network...", address, network);
    let stream = TcpStream::connect(address).await?;
    log::info!("Connected.");

    Ok(stream)
}
