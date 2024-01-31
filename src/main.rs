use tiberius::{Client, Config, AuthMethod};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncWriteCompatExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut config = Config::new();

    println!("llegue hasta aca");

    config.host("localhost");
    config.port(1433);
    config.authentication(AuthMethod::sql_server("SA", "87pacdir-/"));
    config.trust_cert(); // on production, it is not a good idea to do this


    let tcp = TcpStream::connect(config.get_addr()).await?;
    tcp.set_nodelay(true)?;

    // To be able to use Tokio's tcp, we're using the `compat_write` from
    // the `TokioAsyncWriteCompatExt` to get a stream compatible with the
    // traits from the `futures` crate.
    let mut client = Client::connect(config, tcp.compat_write()).await?;

    println!("y hasta aca");

    let _res = client.query("SELECT * FROM ctrlsube.dbo.Registros", &[&-4i32]).await?;
    let row = _res.into_row().await?.unwrap();
    println!("row: {:?}", row);
    Ok(())
}