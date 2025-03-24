#[tokio::main]
async fn run_proxy_server() -> Result<(), Box<dyn std::error::Error>> {
    use bytes::Bytes;
    use reqwest::Client;
    use warp::http::{HeaderMap, Method, Response};
    use warp::Filter;

    // The route pattern captures namespace and name from the path:
    //   /v1/containers/:namespace/:name/proxy
    let route =
        warp::path!("v1" / "containers" / String / String / "proxy")
            .and(warp::method())
            .and(warp::header::headers_cloned())
            .and(warp::body::bytes())
            .and_then(
                |namespace: String,
                 name: String,
                 method: Method,
                 headers: HeaderMap,
                 body: Bytes| async move {
                    let client = Client::new();

                    // Construct the forward URL using the path parameters:
                    let target_url = format!("http://{}.{}.containers.nebu", namespace, name);

                    // Build the outbound request:
                    let mut req_builder = client.request(method, &target_url).body(body);

                    // Forward headers:
                    // (You may want to filter out certain hop-by-hop headers here.)
                    for (key, value) in headers.iter() {
                        // Example filter: skip HTTP/1.1 hop-by-hop headers like transfer-encoding
                        if key.as_str().eq_ignore_ascii_case("transfer-encoding") {
                            continue;
                        }
                        req_builder = req_builder.header(key, value);
                    }

                    // Send the request and handle the response:
                    match req_builder.send().await {
                        Ok(resp) => {
                            let status = resp.status();
                            let headers = resp.headers().clone();
                            let bytes = resp.bytes().await.unwrap_or_else(|_| Bytes::new());

                            // Build a Warp response:
                            let mut builder = Response::builder().status(status);
                            for (key, value) in headers.iter() {
                                builder = builder.header(key, value);
                            }
                            Ok(builder.body(bytes).unwrap())
                        }
                        Err(e) => Err(warp::reject::custom(e)),
                    }
                },
            );

    // Run the server on 0.0.0.0:3030
    warp::serve(route).run(([0, 0, 0, 0], 3030)).await;
    Ok(())
}
