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
use warp::{cors, Filter};
use hpoint_db_pg::pg::{DbPg, PgConfig};
use crate::log::init_log_with_default;
use warp::ws::Message as WarpMessage;
use warp::ws::WebSocket;
use std::convert::Infallible;

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
    let http_server_config = config.clone();
    let websocket_server_config = config.clone();
    let http_server_handle = tokio::spawn(async move {
        if let Err(e) = http_server(http_server_config).await {
            eprintln!("HTTP server error: {:?}", e);
        }
    });

    let websocket_server_handle = tokio::spawn(async move {
        if let Err(e) = websocket_server(websocket_server_config).await {
            eprintln!("WebSocket server error: {:?}", e);
        }
    });

    let _ = tokio::try_join!(http_server_handle, websocket_server_handle);
}

#[derive(Debug, Deserialize, Serialize)]
struct RangeQuery {
    startid: i32,
    lastid: i32,
}

async fn http_server(config: NodeConfig) -> Result<(), Box<dyn std::error::Error>> {
    let addr = config.wport;
    let pg_conn = get_pg_con(PgConfig {
        host: config.db.host,
        port: config.db.port,
        user: config.db.user,
        password: config.db.password,
        dbname: config.db.dbname,
    }).await;
    let arc_con = Arc::new(pg_conn);
    // POST /event -> create a new event
    let create_event = warp::path("event")
        .and(warp::post())
        .and(warp::body::json())
        .map(|new_event: Event| {
            // Here you would normally handle the new event, e.g., save it to a database
            println!("Received event for pst: {:?}", new_event);
            warp::reply::json(&new_event)
        });

    // POST 请求的路由
    let range_query_route = warp::path("range_query")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_db(arc_con.clone()))
        .and_then(handle_range_query);

    //GET /event -> list all events (for simplicity, returning a static example)
    let list_events = warp::path("event")
        .and(warp::get())
        .and(with_db(arc_con.clone()))
        .and_then(handle_get_request);

    // CORS config
    let cors = warp::cors()
        .allow_any_origin()
        .allow_method("POST")
        .allow_method("GET")
        .allow_header("content-type");

    // Create a warp server and bind it to an address with CORS enabled
    let routes = create_event.or(list_events).or(range_query_route).with(cors);
    let warp_addr = addr.parse::<std::net::SocketAddr>()?;
    let wserver = warp::serve(routes);
    println!("addr {}", warp_addr);
    wserver.run(warp_addr).await;

    Ok(())
}

fn with_db(db: Arc<DbPg>) -> impl Filter<Extract=(Arc<DbPg>, ), Error=std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}


#[derive(Debug, Deserialize, Serialize)]
struct Response {
    message: Vec<hpoint_db_pg::pg::Event>,
}

async fn handle_get_request(db: Arc<DbPg>) -> Result<impl warp::Reply, Infallible> {
    let events = db.query_all_from_event().await;
    let response = Response { message: events };
    Ok(warp::reply::json(&response))
}

async fn handle_range_query(range_query: RangeQuery, db_pg: Arc<DbPg>) -> Result<impl warp::Reply, Infallible> {
    let events = db_pg.query_event_by_range_id(range_query.startid, range_query.lastid).await;
    let response = Response { message: events };
    Ok(warp::reply::json(&response))
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
    pub wport: String,
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

    // let cors = cors()
    //     .allow_any_origin()
    //
    //     .allow_methods(vec!["GET", "POST", "OPTIONS"])
    //     .allow_headers(vec!["Authorization",
    //                         "Content-Type","User-Agent", "Sec-Fetch-Mode", "Referer", "Origin", "Access-Control-Request-Method", "Access-Control-Request-Headers",
    //                         "Origin"])
    //     .build();
    //
    // let con_for_web = arc_con.clone();
    // let routes = warp::path("event")
    //     .and(warp::ws())
    //     .map(move |ws: warp::ws::Ws| {
    //         let arc_con = con_for_web.clone();
    //         ws.on_upgrade(move |websocket| {
    //             handle_connection_outside(websocket, arc_con)
    //         })
    //     })
    //     .with(cors).with(warp::log("cors test"));

    // let server = warp::serve(routes);
    // let warp_addr = waddr.parse::<std::net::SocketAddr>()?;
    // tokio::spawn(server.run(warp_addr));
    // let con_for_web = arc_con.clone();
    // // POST /event -> create a new event
    // let create_event = warp::path("event")
    //     .and(warp::post())
    //     .and(warp::body::json())
    //     .map( |new_event: Event| {
    //         // Here you would normally handle the new event, e.g., save it to a database
    //         let e_list = con_for_web.query_all_from_event();
    //         async move {
    //             match e_list.await {
    //                 Vec { .. } => {
    //                     println!("Received event: {:?}", new_event);
    //                     warp::reply::json(&e_list)
    //                 }
    //             }
    //         }
    //         // println!("Received event: {:?}", new_event);
    //         // warp::reply::json(&e_list)
    //     });
    //
    // let cors = warp::cors()
    //     .allow_any_origin()
    //     .allow_method("POST")
    //     .allow_header("content-type");
    //
    // // Create a warp server and bind it to an address with CORS enabled
    // let routes = create_event.with(cors);
    // let warp_addr = waddr.parse::<std::net::SocketAddr>()?;
    // let wserver = warp::serve(routes);
    // tokio::spawn(  wserver.run(warp_addr));


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
        println!("{}", msg);
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

async fn handle_connection_outside(ws: WebSocket, conn: Arc<DbPg>) {
    let (mut write, mut read) = ws.split();
    conn.create_event_table("event".to_string()).await;
    while let Some(result) = read.next().await {
        match result {
            Ok(msg) => {
                if msg.is_text() {
                    let text = msg.to_str().unwrap();
                    let message: Result<Message, _> = serde_json::from_str(&text);
                    if let Ok(message) = message {
                        match message {
                            Message::InsertEvent(event) => {
                                // Handle insert event
                                println!("Insert event: {:?}", event);
                                let id = conn.get_latest_id_by_table_name("event".to_string()).await;
                                println!("read latest id is {}", id);
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
                                write.send(WarpMessage::text(serialized_response)).await.unwrap();
                            }
                            Message::QueryEvent => {
                                println!("query event");
                                let e_list = conn.query_all_from_event().await;
                                // Handle query event and send response
                                let response = Message::Response(e_list);
                                let serialized_response = serde_json::to_string(&response).unwrap();
                                write.send(WarpMessage::text(serialized_response)).await.unwrap();
                            }
                            _ => {}
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
        }
    }
}