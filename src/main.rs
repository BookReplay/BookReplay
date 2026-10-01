#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        None => bookreplay_api::run().await,
        Some("reset-owner-password") => bookreplay_api::reset_owner_password().await,
        Some("healthcheck") => bookreplay_api::healthcheck().await,
        Some(_) => Err("usage: bookreplay [reset-owner-password | healthcheck]".into()),
    }
}
