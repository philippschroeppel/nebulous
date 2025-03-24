use anyhow::Error;
use anyhow::Result;
use hickory_server::authority::{Catalog, ZoneType};
use hickory_server::proto::rr::Name;
use hickory_server::server::ServerFuture;
use hickory_server::store::file::{FileAuthority, FileConfig};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::UdpSocket;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Build our zone configuration for a zone file, e.g., example.com.zone
    let file_config = FileConfig {
        zone_file_path: "example.com.zone".into(),
    };

    let origin = Name::from_str("example.com.")?;
    let file_authority = FileAuthority::try_from_config(
        origin,
        ZoneType::Primary,
        false,
        None,
        &file_config,
        #[cfg(feature = "__dnssec")]
        None,
    )
    .map_err(Error::msg)?;

    // 3. Create a Catalog and insert our new authority
    let mut catalog = Catalog::new();

    // "example.com." must match the $ORIGIN in your zone file
    let origin = "example.com.".parse()?;
    catalog.upsert(
        origin,
        vec![Arc::new(file_authority)], // build Vec<Arc<dyn AuthorityObject>>
    );

    // 4. Create and configure the server
    //
    // ServerFuture can take a Catalog (or any RequestHandler) to handle DNS queries
    let mut server_future = ServerFuture::new(catalog);

    // 5. Register the address and port on which to listen
    // Example: 127.0.0.1:5300
    let addr = SocketAddr::from_str("127.0.0.1:5300")?;
    let udp_socket = UdpSocket::bind(addr).await?;
    server_future.register_socket(udp_socket);

    // 6. Run the server until shutdown
    println!("Hickory DNS server listening on {}", addr);
    server_future.block_until_done().await?;
    Ok(())
}
