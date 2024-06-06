mod error;
mod log;
mod runtime;
mod build;
mod pointservice;


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
use tonic::{async_trait, IntoRequest};
use hpoint_db_pg::pg::{DbPg, PgConfig};
use crate::log::init_log_with_default;
use crate::pointservice::point_service_server::{PointService, PointServiceServer};
use tonic::{transport::Server, Request, Response, Status};
use crate::pointservice::{Event, InsertRequest, InsertResponse, QueryEventRequest, ResponseEvents};
use chrono::prelude::*;


macro_rules! println_with_time {
    ($($arg:tt)*) => {{
        let current_time = Local::now();
        println!("[{}] {}", current_time.format("%Y-%m-%d %H:%M:%S"), format_args!($($arg)*));
    }}
}

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
    println_with_time!("Config: {:?}", config);
    info!("Config: {:?}", config);
    rpc_server(config).await.unwrap();
}

async fn rpc_server(config: NodeConfig) -> Result<(), Box<dyn std::error::Error>> {
    let addr = config.port.parse()?;
    let pg_conn = get_pg_con(PgConfig {
        host: config.db.host,
        port: config.db.port,
        user: config.db.user,
        password: config.db.password,
        dbname: config.db.dbname,
    }).await;
    let arc_con = Arc::new(pg_conn);
    arc_con.clone().create_event_table("sglk_event".to_string()).await;
    let point_service = MyPointService::init(arc_con.clone());

    println_with_time!("PointServiceServer listening on {}", addr);

    Server::builder()
        .add_service(PointServiceServer::new(point_service))
        .serve(addr)
        .await?;

    Ok(())
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

#[derive(Debug)]
pub struct MyPointService {
    conn: Arc<DbPg>,
}

impl MyPointService {
    pub fn init(conn: Arc<DbPg>) -> MyPointService {
        MyPointService {
            conn,
        }
    }
}

#[async_trait]
impl PointService for MyPointService {
    async fn insert_event(
        &self,
        request: Request<InsertRequest>,
    ) -> Result<Response<InsertResponse>, Status> {
        println_with_time!("Got a request: {:?}", request);

        let id = self.conn.get_latest_id_by_table_name("event".to_string()).await;
        println_with_time!("read latest id is {}", id);
        info!("read latest id is {}", id);
        let event = request.into_inner().event.unwrap();
        self.conn.event_insert(hpoint_db_pg::pg::Event {
            id: id + 1,
            pk_user: event.pk_user,
            pk_owner: event.pk_owner,
            event_meta: event.event_meta,
            event_type: event.event_type,
            point_amount: 1,
        }).await;

        let reply = InsertResponse { insert_id: (id + 1).to_string() };

        Ok(Response::new(reply))
    }

    async fn query_event(
        &self,
        request: Request<QueryEventRequest>,
    ) -> Result<Response<ResponseEvents>, Status> {
        println_with_time!("Got a request: {:?}", request);
        info!("receive query event request");
        let e_list = self.conn.query_all_from_event().await;
        let reply = ResponseEvents { events:e_list };

        Ok(Response::new(reply))
    }
}

pub async fn get_pg_con(conf: PgConfig) -> hpoint_db_pg::pg::DbPg {
    hpoint_db_pg::pg::init(conf).await
}
