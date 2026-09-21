#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        None => bookreplay_api::run().await.map_err(|_| {
            "startup or server operation failed; check configuration and database availability"
                .into()
        }),
        Some("reset-owner-password") => bookreplay_api::reset_owner_password().await,
        Some(_) => Err("usage: bookreplay [reset-owner-password]".into()),
    }
}
