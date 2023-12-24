use std::fs::read_to_string;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::select;
use tokio::task::{JoinSet};


#[tokio::main(flavor = "current_thread")]
async fn main() {
    let config = Box::new(getConfigPath()).leak();
    let mut set = JoinSet::new();
    for connConf in config {
        set.spawn(async move {
            setupProxy(&connConf.0, &connConf.1).await.unwrap_or_else(|e| {
                println!("Failed to listen to {}, Error {}", &connConf.0, e);
            });
        });
    }
    while let Some(res) = set.join_next().await {
    }
}

async fn setupProxy(src: &'static str, dst: &'static str) -> io::Result<()> {
    println!("src {} -> dest {}", src, dst);
    let listener = TcpListener::bind(src).await?;
    let counter = Arc::new(AtomicI64::new(0));
    loop {
        let client = listener.accept().await?.0;
        let server = match TcpStream::connect(dst).await {
            Ok(server) => server,
            Err(e) => {
                println!("Failed to connect to {}, Error {}", dst, e);
                continue
            }
        };
        println!("New connection {} -> {}. Overall {}", src, dst, counter.fetch_add(1, Ordering::Relaxed));
        let spanCounter = counter.clone();
        tokio::spawn(async move {
            handleConnection(client, server).await.unwrap_or_else(|error| {
                let counter = spanCounter.fetch_add(-1, Ordering::Relaxed);
                println!("Connection dropped {} -> {}. Overall {} {}", src, dst, counter - 1, error)
            });
        });
    }
}

async fn handleConnection(mut client: TcpStream, mut server: TcpStream) -> Result<(), io::Error> {
    let mut bufferServer = [0u8; 1024];
    let mut bufferClient = [0u8; 1024];
    loop {
        select! {
            result = server.read(&mut bufferServer) => {
                match result {
                    Ok(size) => client.write_all(&mut bufferServer[..size]).await?,
                    Err(e) => return Err(e)
                }
            },
            result = client.read(&mut bufferClient) => {
                match result {
                    Ok(size) => server.write_all(&mut bufferClient[..size]).await?,
                    Err(e) => return Err(e)
                }
            }
        }
    }
}

fn getConfigPath() -> Vec<(String, String)> {
    let configStr : String;
    if (std::env::args().count() == 2) {
        configStr = std::env::args().nth(1).unwrap();
    } else {
        configStr = home::home_dir().unwrap().join(".proxy/config").to_str().unwrap().to_string();
    }
    let config = read_to_string(configStr).unwrap();
    config.lines()
        .map(|l| {l.split_once(' ').unwrap()})
        .map(|l| {(l.0.to_string(), l.1.to_string())})
        .collect()
}