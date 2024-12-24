# api-client

![Build Status](https://github.com/demostf/api-client/workflows/CI/badge.svg)

Rust api client for demos.tf

## Example

```rust
use demostf_client::{ListOrder, ListParams, ApiClient};

#[tokio::main]
async fn main() -> Result<(), demostf_client::Error> {
    let client = ApiClient::new();

    let demos = client.list(ListParams::default().with_order(ListOrder::Ascending), 1).await?;

    for demo in demos {
        println!("{}: {}", demo.id, demo.name);
    }
    Ok(())
}
```
