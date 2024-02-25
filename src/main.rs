use tiberius::{Client, Config, AuthMethod};
use tiberius::EncryptionLevel::{NotSupported};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncWriteCompatExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut config = Config::new();

    config.host("localhost");
    config.port(1433);
    config.database("ctrlsube");
    config.authentication(AuthMethod::sql_server("sa", "87pacdir-/"));
    config.trust_cert(); // on production, it is not a good idea to do this
    config.encryption(NotSupported);

    let tcp = TcpStream::connect(config.get_addr()).await?;
    tcp.set_nodelay(true)?;

    let mut client = Client::connect(config, tcp.compat_write()).await.expect("Failed!");

    let _res = client.query("SELECT * FROM Registros", &[&-4i32]).await?;
    let results = _res.into_results().await?;

    println!("results: {:?}", results);

    for row in results.get(0).unwrap() {
        println!("row: {:?}", row);
    }

    Ok(())
}