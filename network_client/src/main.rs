use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use url::Url;
use serde::{Deserialize, Serialize};
use bincode::deserialize;


#[derive(Serialize, Deserialize, Debug)]
enum Message {
    InsertEvent(Event),
    QueryEvent,
    Response(Vec<REvent>),
    ResponseWriteId(Id),
}

type Id = i32;

#[derive(Serialize, Deserialize, Debug)]
pub struct REvent {
    pub id: i32,
    pub pk_owner: String,
    pub pk_user: String,
    pub event_meta: Vec<u8>,  // json utf8 ?
    pub event_type: String,
    pub point_amount: i32,
}

#[derive(Serialize, Deserialize, Debug)]
struct Event {
    pk_owner: String,
    pk_user: String,
    event_meta: Vec<u8>,
    event_type: String,
}


#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct EventMeta {
    pub event_type: i32,
    pub timestamp: i64,
    pub address: String,
    pub project_name: String,
    pub sign: String,
    pub sign_method: String,
    pub event_date: String,
    pub duration: i32,
}
async fn websocket_client() -> Result<(), Box<dyn std::error::Error>> {
    let url = Url::parse("ws://173.199.118.240:6666").unwrap();
    //let url = Url::parse("ws://173.199.118.240:6666").unwrap();
    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    let (mut write, mut read) = ws_stream.split();

   //Example: Insert Event
    let event = Event {
        pk_owner: "owner1".to_string(),
        pk_user: "user1".to_string(),
        event_meta: vec![1, 2, 3],
        event_type: "type1".to_string(),
    };
    let message = Message::InsertEvent(event);
    //  let message =Message::QueryEvent;
    // let serialized_message = serde_json::to_string(&message)?;
    // let message = Message::QueryEvent;
     let serialized_message = serde_json::to_string(&message)?;

    write.send(WsMessage::Text(serialized_message)).await?;

    // Example: Read response
    while let Some(msg) = read.next().await {
        let msg = msg?;
        if let WsMessage::Text(text) = msg {
            let response: Message = serde_json::from_str(&text)?;
            match response {
                Message::Response(events) => {
                  //  println!("Received events: {:?}", events[events.len()-1].clone());
                    let decoded: EventMeta = deserialize(&*events[events.len() - 4].event_meta).expect("failed to decode");

                    // 打印反序列化后的结构体
                    println!("{:?}", decoded);
                    write.close().await.expect("TODO: panic message");
                }
                Message::ResponseWriteId(id) => {
                    println!("Received write id : {}", id);
                    write.close().await.expect("TODO: panic message");
                }

                _ => {
                    write.close().await.expect("TODO: panic message");
                }
            }
        }
    }

    Ok(())
}


#[tokio::main]
async fn main() {
    websocket_client().await.unwrap();
}
