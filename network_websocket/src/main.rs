use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use ::log::info;
use futures::{AsyncWriteExt, SinkExt, StreamExt};
use crate::runtime::takio_runtime::block_forever_on;
use structopt::StructOpt;
use crate::error::{ConfigError, ConfigResult};
use serde::Deserialize;
use serde::Serialize;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use serde_json::Result as SerdeResult;
use hpoint_db_pg::pg::{DbPg, PgConfig};
use crate::log::init_log_with_default;

mod runtime;
mod error;
mod log;

#[tokio::main]
async fn main() {
    block_forever_on(async_main());
}

#[derive(StructOpt)]
struct Cli {
    #[structopt(short = "c", long = "config", parse(from_os_str), help = "Yaml file only")]
    config_path: std::path::PathBuf,
}

async fn async_main() {
    init_log_with_default();
    let args = Cli::from_args();
    let config = load_config(args.config_path.clone()).unwrap();
    println!("Config: {:?}", config);
    info!("Config: {:?}", config);
    websocket_server(config).await.unwrap();
}

fn load_config(path: PathBuf) -> ConfigResult<NodeConfig> {
    let p: &Path = path.as_ref();
    let config_yaml = std::fs::read_to_string(p).map_err(|err| match err {
        e @ std::io::Error { .. } if e.kind() == std::io::ErrorKind::NotFound => {
            ConfigError::ConfigMissing(path.into())
        }
        _ => err.into(),
    })?;
    serde_yaml::from_str(&config_yaml).map_err(ConfigError::SerializationError)
}

#[derive(Clone, Deserialize, Serialize, Debug, Default)]
pub struct NodeConfig {
    pub port: String,
    pub db: PgDbConfig,
}

#[derive(Clone, Deserialize, Serialize, Debug, Default)]
pub struct PgDbConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub dbname: String,
}

async fn websocket_server(config: NodeConfig) -> Result<(), Box<dyn std::error::Error>> {
    let addr = config.port;
    let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
    let pg_conn = get_pg_con(PgConfig {
        host: config.db.host,
        port: config.db.port,
        user: config.db.user,
        password: config.db.password,
        dbname: config.db.dbname,
    }).await;
    let arc_con = Arc::new(pg_conn);
    while let Ok((stream, _)) = listener.accept().await {
        let arc_per = arc_con.clone();
        tokio::spawn(handle_connection(stream, arc_per));
    }

    Ok(())
}

async fn handle_connection(stream: TcpStream, conn: Arc<DbPg>) {
    let ws_stream = accept_async(stream).await.expect("Failed to accept");
    let (mut write, mut read) = ws_stream.split();
    conn.create_event_table("event".to_string()).await;
    while let Some(msg) = read.next().await {
        let msg = msg.expect("Failed to read message");
        if let WsMessage::Text(text) = msg {
            let message: SerdeResult<Message> = serde_json::from_str(&text);
            if let Ok(message) = message {
                match message {
                    Message::InsertEvent(event) => {
                        // Handle insert event
                        println!("Insert event: {:?}", event);
                        info!("Insert event: {:?}", event);
                        let id = conn.get_latest_id_by_table_name("event".to_string()).await;
                        println!("read latest id is {}", id);
                        info!("read latest id is {}", id);
                        conn.event_insert(hpoint_db_pg::pg::Event {
                            id: id + 1,
                            pk_user: event.pk_user,
                            pk_owner: event.pk_owner,
                            event_meta: event.event_meta,
                            event_type: event.event_type,
                            point_amount: 1,
                        }).await;
                        let response = Message::ResponseWriteId(id + 1);
                        let serialized_response = serde_json::to_string(&response).unwrap();
                        write.send(WsMessage::Text(serialized_response)).await.unwrap();
                    }
                    Message::QueryEvent => {
                        println!("query event");
                        info!("receive query event request");
                        let e_list = conn.query_all_from_event().await;
                        // Handle query event and send response
                        let response = Message::Response(e_list);
                        let serialized_response = serde_json::to_string(&response).unwrap();
                        write.send(WsMessage::Text(serialized_response)).await.unwrap();
                    }
                    _ => {}
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
enum Message {
    InsertEvent(Event),
    QueryEvent,
    Response(Vec<hpoint_db_pg::pg::Event>),
    ResponseWriteId(Id),
}

type Id = i32;

#[derive(Serialize, Deserialize, Debug)]
struct Event {
    pk_owner: String,
    pk_user: String,
    event_meta: Vec<u8>,
    event_type: String,
}

pub async fn get_pg_con(conf: PgConfig) -> hpoint_db_pg::pg::DbPg {
    hpoint_db_pg::pg::init(conf).await
}
