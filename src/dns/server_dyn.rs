// use anyhow::Error;
// use anyhow::Result;
// use hickory_server::authority::{Catalog, ZoneType};
// use hickory_server::proto::rr::rdata;
// use hickory_server::proto::{
//     rr::record_type::RecordType, rr::LowerName, rr::Name, rr::RData, rr::Record,
// };
// use hickory_server::server::ServerFuture;
// use hickory_server::store::in_memory::InMemoryAuthority;
// use std::collections::BTreeMap;
// use std::net::SocketAddr;
// use std::str::FromStr;

// use hickory_server::authority::AuthorityObject;
// use std::any::Any;
// use std::sync::{Arc, Mutex};
// use tokio::net::UdpSocket;
// use warp::{Filter, Rejection, Reply};

// use hickory_server::server::RequestHandler; // Trait implemented by Catalog
// use hickory_server::server::{Request, ResponseHandler, ResponseInfo};

// struct SharedCatalog(Arc<Catalog>);

// #[async_trait::async_trait]
// impl RequestHandler for SharedCatalog {
//     async fn handle_request<R: ResponseHandler>(
//         &self,
//         request: &Request,
//         response_handle: R,
//     ) -> ResponseInfo {
//         // Forward to the inner Catalog
//         self.0.handle_request(request, response_handle).await
//     }
// }

// trait AuthorityObjectEx {
//     fn as_any_mut(&mut self) -> &mut dyn Any;
// }

// impl<T: 'static + AuthorityObject> AuthorityObjectEx for T {
//     fn as_any_mut(&mut self) -> &mut dyn Any {
//         self
//     }
// }

// trait AsInMemory {
//     fn as_in_memory(&mut self) -> Option<&mut InMemoryAuthority>;
// }

// impl<T: AuthorityObjectEx> AsInMemory for T {
//     fn as_in_memory(&mut self) -> Option<&mut InMemoryAuthority> {
//         self.as_any_mut().downcast_mut::<InMemoryAuthority>()
//     }
// }

// /// A shared type that holds our Catalog so we can update it with new records
// /// or remove records dynamically.
// struct DnsState {
//     catalog: Mutex<Catalog>,
//     origin: Name,
// }

// #[tokio::main]
// async fn main() -> Result<()> {
//     // 1. Build an in-memory authority for "example.com."
//     let origin = Name::from_str("example.com.")?;
//     let mut in_mem_auth = Arc::new(InMemoryAuthority::new(
//         origin.clone(),
//         BTreeMap::new(),
//         ZoneType::Primary,
//         false,
//     ))
//     .map_err(|e| anyhow::anyhow!(e))?;

//     // Optionally, seed some initial records:
//     // e.g., an A record for "www.example.com."
//     let seed_record = Record::from_rdata(
//         Name::from_str("www.example.com.")?,
//         3600,
//         RData::A("127.0.0.1".parse().unwrap()),
//     );
//     if !in_mem_auth.upsert(seed_record, 0).await {
//         return Err(anyhow::anyhow!("Failed to upsert record"));
//     }

//     // 2. Create a Catalog and insert our in-memory authority
//     let mut catalog = Catalog::new();
//     catalog.upsert(LowerName::new(&origin), vec![Arc::new(in_mem_auth)]);

//     // Create a shared, thread-safe DnsState
//     let shared_catalog = Arc::new(DnsState {
//         catalog: Mutex::new(catalog),
//         origin,
//     });

//     let catalog = Catalog::new(); // or however you build it
//     let arc_catalog = Arc::new(catalog);
//     let shared = SharedCatalog(arc_catalog.clone());
//     let mut server_future = ServerFuture::new(shared);

//     // 3. Create and configure the DNS server
//     // let mut server_future = ServerFuture::new(shared_catalog.catalog.lock().unwrap().clone());

//     // 4. Register the address and port on which to listen
//     let addr = SocketAddr::from_str("127.0.0.1:5300")?;
//     let udp_socket = UdpSocket::bind(addr).await?;
//     server_future.register_socket(udp_socket);

//     // 5. Spin up an HTTP/REST endpoint to allow management of DNS records.
//     //    For example: POST /add_record ; POST /remove_record
//     let http_addr = ([127, 0, 0, 1], 8080);

//     let add_route = warp::path!("add_record")
//         .and(warp::post())
//         .and(warp::body::json())
//         .and(with_dns_state(shared_catalog.clone()))
//         .and_then(add_record_handler);

//     let remove_route = warp::path!("remove_record")
//         .and(warp::post())
//         .and(warp::body::json())
//         .and(with_dns_state(shared_catalog.clone()))
//         .and_then(remove_record_handler);

//     let routes = add_route.or(remove_route);

//     // Spawn the HTTP server.
//     tokio::spawn(async move {
//         println!("HTTP API listening on {:?}", http_addr);
//         warp::serve(routes).run(http_addr).await;
//     });

//     // 6. Run the DNS server until shutdown
//     println!("Hickory DNS server listening on {}", addr);
//     server_future.block_until_done().await?;
//     Ok(())
// }

// /// Helper filter to clone an Arc<DnsState> into each handler.
// fn with_dns_state(
//     dns_state: Arc<DnsState>,
// ) -> impl Filter<Extract = (Arc<DnsState>,), Error = std::convert::Infallible> + Clone {
//     warp::any().map(move || dns_state.clone())
// }

// /// Data structure for adding/removing a record.
// /// You might want to refine this based on your needs.
// #[derive(serde::Deserialize)]
// struct RecordRequest {
//     pub name: String,        // e.g. "www.example.com."
//     pub record_type: String, // e.g. "A", "CNAME", "TXT", etc.
//     pub rdata: String,       // e.g. "127.0.0.1", or "alias.example.com."
//     pub ttl: u32,
// }

// /// Handler to add a record (POST /add_record).
// async fn add_record_handler(
//     req: RecordRequest,
//     state: Arc<DnsState>,
// ) -> Result<impl Reply, Rejection> {
//     let mut catalog = state.catalog.lock().unwrap();

//     // Get a reference to our in-memory authority from the Catalog
//     // by matching the origin "example.com." we used above.
//     if let Some(mut auths) = catalog.remove(&LowerName::new(&state.origin)) {
//         // We only stored one authority in the vec, so we can unwrap it:
//         if let Some(auth) = Arc::get_mut(&mut auths[0]) {
//             let auth_ex = auth as &mut dyn AuthorityObjectEx;
//             if let Some(in_mem_auth) = auth_ex.as_in_memory() {
//                 // Build the Record
//                 let name = Name::from_str(&req.name).map_err(|_| warp::reject())?;
//                 let record_type =
//                     RecordType::from_str(&req.record_type).map_err(|_| warp::reject())?;

//                 // Build RData. You’d parse it based on record_type (A, CNAME, etc.)
//                 let rdata = match record_type {
//                     RecordType::A => RData::A(req.rdata.parse().map_err(|_| warp::reject())?),
//                     RecordType::AAAA => RData::AAAA(req.rdata.parse().map_err(|_| warp::reject())?),
//                     RecordType::CNAME => {
//                         let cname_name = Name::from_str(&req.rdata).map_err(|_| warp::reject())?;
//                         RData::CNAME(rdata::CNAME(cname_name))
//                     }
//                     // Add other branches as needed
//                     _ => {
//                         eprintln!("Unsupported record type for dynamic insertion.");
//                         return Err(warp::reject());
//                     }
//                 };

//                 let record = Record::from_rdata(name, req.ttl, rdata);

//                 // Insert the new record
//                 in_mem_auth.add_record(record, 0).map_err(|e| {
//                     eprintln!("Error adding record: {:?}", e);
//                     warp::reject()
//                 })?;

//                 // The server updates automatically on the next query
//                 return Ok(format!("Record added: {}", req.name));
//             }
//         }
//     }

//     Err(warp::reject())
// }

// /// Handler to remove a record (POST /remove_record).
// async fn remove_record_handler(
//     req: RecordRequest,
//     state: Arc<DnsState>,
// ) -> Result<impl Reply, Rejection> {
//     let mut catalog = state.catalog.lock().unwrap();

//     if let Some(auths) = catalog.get_mut(&state.origin) {
//         if let Some(auth) = Arc::get_mut(&mut auths[0]) {
//             if let Some(in_mem_auth) = auth.as_in_memory() {
//                 let name = Name::from_str(&req.name).map_err(|_| warp::reject())?;
//                 let record_type =
//                     RecordType::from_str(&req.record_type).map_err(|_| warp::reject())?;
//                 in_mem_auth.remove_record(&name, record_type);

//                 // The server updates automatically on the next query
//                 return Ok(format!("Record removed: {}", req.name));
//             }
//         }
//     }

//     Err(warp::reject())
// }
