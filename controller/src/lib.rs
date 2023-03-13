pub mod app;
pub mod manager;

#[cfg(test)]
mod tests {
    use std::error::Error;

    use tokio::runtime::Runtime;
    use tokio_postgres::NoTls;

    use crate::app::job::JobState;

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

    #[test]
    fn test_insert_db() -> Result<(), Box<dyn Error>> {
        let rt = Runtime::new()?;
        rt.block_on(async {
            let (client, connection) =
                tokio_postgres::connect("host=localhost user=simepi password=simepi", NoTls)
                    .await
                    .unwrap();
            tokio::spawn(async move {
                if let Err(e) = connection.await {
                    eprintln!("connection error: {}", e);
                }
            });
            let rows = client
                .query(
                    r#"
                    INSERT INTO
                        job (id, state)
                        VALUES (DEFAULT, $1)
                        RETURNING id
                    "#,
                    &[&JobState::Created],
                )
                .await
                .unwrap();

            println!("{rows:?}");
            let r: uuid::Uuid = rows[0].get(0);
            println!("{r}");
        });
        Ok(())
    }
}
