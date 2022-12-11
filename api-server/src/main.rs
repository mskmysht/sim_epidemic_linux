mod api;

use poem::{listener::TcpListener, Result};

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    poem::Server::new(TcpListener::bind("0.0.0.0:3000"))
        .run(api::create_app().await)
        .await
}
