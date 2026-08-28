#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookreplay_api::run().await
}
