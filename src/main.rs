#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    rekindle_api::run().await
}
