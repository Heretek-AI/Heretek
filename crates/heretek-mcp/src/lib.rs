pub mod doctor;
pub mod server;
pub mod tools;

pub async fn serve_stdio() -> anyhow::Result<()> {
    server::serve_stdio().await
}
