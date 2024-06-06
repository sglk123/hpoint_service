mod pointservice;

use tonic::transport::Channel;
use tonic::Request;
use crate::pointservice::{Event, InsertRequest, QueryEventRequest};
use crate::pointservice::point_service_client::PointServiceClient;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = PointServiceClient::connect("http://127.0.0.1:6666").await?;

    // Send InsertEvent request
    let request = tonic::Request::new(InsertRequest {
        event: Some(Event {
            pk_owner: "owner1".to_string(),
            pk_user: "user1".to_string(),
            event_meta: vec![1, 2, 3, 4],
            event_type: "type1".to_string(),
        }),
    });

    let response = client.insert_event(request).await?;

    println!("Response from InsertEvent: {:?}", response.into_inner().insert_id);

    // Send QueryEvent request
    let request = tonic::Request::new(QueryEventRequest {});

    let response = client.query_event(request).await?;

    println!("Response from QueryEvent: {:?}", response.into_inner().events);

    Ok(())
}
