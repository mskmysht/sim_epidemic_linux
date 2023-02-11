pub mod api_server;
pub mod management;
pub mod repl_handler;
mod types;

#[cfg(test)]
mod tests {
    #[test]
    fn connect_db_test() {
        use tokio_postgres::NoTls;
        async fn f() -> Result<(), tokio_postgres::Error> {
            let (client, connection) =
                tokio_postgres::connect("host=localhost user=simepi password=simepi", NoTls)
                    .await?;

            tokio::spawn(async move {
                if let Err(e) = connection.await {
                    eprintln!("connection error: {}", e);
                }
            });

            for r in client.query("SELECT * FROM jobstate", &[]).await? {
                println!("found person: {r:?}");
            }
            Ok(())
        }

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(f()).unwrap();
    }
}
