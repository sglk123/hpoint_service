use std::fmt::format;
use std::sync::Arc;
use bb8_postgres::{PostgresConnectionManager, tokio_postgres};
use bb8_postgres::bb8::{Pool, PooledConnection, RunError};
use postgres::NoTls;
use crate::{pg_insert, pg_select};
use tokio_postgres::Error;
use serde::{Deserialize, Serialize};

type ConPool = Arc<Pool<PostgresConnectionManager<NoTls>>>;
type Con<'a> = PooledConnection<'a, PostgresConnectionManager<NoTls>>;

const POOL_SIZE: u32 = 15;

pub struct InitDbConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
}

/// todo ownership cant work
async fn create_database(conf: InitDbConfig) -> Result<(), Error> {
    let (client, connection) = tokio_postgres::connect(
        &format!("host={} user={} password={} port={}",
                 &conf.host, &conf.user, &conf.password, &conf.port),
        NoTls,
    )
        .await?;

    client
        .execute(
            "CREATE DATABASE sglksample2", // 创建数据库的 SQL 命令
            &[],
        )
        .await?;

    connection.await?;
    Ok(())
}

pub struct PgConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub dbname: String,
}


// global db context
pub struct DbPg {
    pub pool: ConPool,
    pub conf: PgConfig,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Event {
    pub id: i32,
    pub pk_owner: String,
    pub pk_user: String,
    pub event_meta: Vec<u8>,  // json utf8 ?
    pub event_type: String,
    pub point_amount: i32,
}

pub async fn init(conf: PgConfig) -> DbPg {
    let mut pg_config = tokio_postgres::Config::new();
    pg_config.host(&conf.host);
    pg_config.port(conf.port);
    pg_config.user(&conf.user);
    pg_config.password(&conf.password);
    pg_config.dbname(&conf.dbname);

    let manager = PostgresConnectionManager::new(pg_config, NoTls);
    let pool = Arc::new(Pool::builder().max_size(POOL_SIZE).build(manager).await.unwrap());
    DbPg {
        pool,
        conf,
    }
}

impl DbPg {
    pub async fn create_event_table(&self, table: String) {
        let mut conn = self.pool.get().await.unwrap();
        let _ = conn
            .execute(&format!(
                "CREATE TABLE {}(
            id      SERIAL PRIMARY KEY,
            pk_owner    TEXT NOT NULL,
            pk_user    TEXT NOT NULL,
            event_meta    BYTEA,
            event_type    TEXT NOT NULL,
            point_amount    INTEGER NOT NULL
        )", table),
                     &[],
            )
            .await;
    }
    pub async fn event_insert(&self, event: Event) {
        let conn = self.pool.get().await.unwrap();
        let _ = pg_insert!(conn, event, {
            id: event.id,
            pk_owner: event.pk_owner,
            pk_user: event.pk_user,
            event_meta: event.event_meta,
            event_type: event.event_type,
            point_amount: event.point_amount,
        });
    }

    pub async fn get_latest_id_by_table_name(&self, table_name: String) -> i32 {
        let mut conn = self.pool.get().await.unwrap();
        let id = conn.query(&format!("SELECT id FROM {} ORDER BY id DESC LIMIT 1", table_name), &[]).await.unwrap();
        if id.len() == 0 {
            return 0;
        }
        let latest_id: i32 = id[0].try_get(0).unwrap();
        latest_id
    }

    pub async fn query_all_from_event(&self) -> Vec<Event> {
        let mut conn = self.pool.get().await.unwrap();
        let rows = conn.query("SELECT * FROM event", &[]).await.unwrap();
        let mut events = Vec::new();
        for row in rows.iter() {
            let id: i32 = row.try_get(0).unwrap();
            let pk_owner: String = row.try_get(1).unwrap();
            let pk_user: String = row.try_get(2).unwrap();
            let event_meta: Vec<u8> = row.try_get(3).unwrap();
            let event_type: String = row.try_get(4).unwrap();
            let point_amount: i32 = row.try_get(5).unwrap();
            println!("{} {} {} {:?} {} {}", id, pk_owner, pk_user, event_meta, event_type, point_amount);
            let event = Event {
                id,
                pk_owner,
                pk_user,
                event_meta,
                event_type,
                point_amount,
            };

            events.push(event);
        }

        events
    }
    pub async fn query_test(&self) {
        let mut conn = self.pool.get().await.unwrap();
        // let tx = conn.transaction().await.unwrap();
        let rows1 = pg_select!(conn, person, { id, name, data} where name = Nick).await.unwrap();
        //let rows1 = conn.query("SELECT id, name, data FROM person WHERE id = 1", &[]).await.unwrap();
        // let rows = conn
        //     .query("SELECT id, name, data FROM person", &[])
        //     .await.unwrap();

        // Process the query results for test
        for row in rows1.iter() {
            let id: i32 = row.try_get(0).unwrap();
            let name: &str = row.try_get(1).unwrap();
            let data: Option<Vec<u8>> = row.try_get(2).unwrap();
            println!("{} {} {:?}", id, name, data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn event_query_all() {
        // Define a test configuration
        let test_conf = PgConfig {
            host: "localhost".to_string(),
            port: 5432,
            user: "postgres".to_string(),
            password: "123".to_string(),
            dbname: "sglk_sample1".to_string(),
        };
        let db_pg = init(test_conf).await;
        db_pg.query_all_from_event().await;
    }


    #[tokio::test]
    async fn event_insert() {
        // Define a test configuration
        let test_conf = PgConfig {
            host: "localhost".to_string(),
            port: 5432,
            user: "postgres".to_string(),
            password: "123".to_string(),
            dbname: "sglk_sample1".to_string(),
        };
        let db_pg = init(test_conf).await;
        db_pg.event_insert(Event {
            id: 2,
            pk_owner: "sglk".to_string(),
            pk_user: "sglk".to_string(),
            event_meta: vec![1, 2],
            event_type: "genius".to_string(),
            point_amount: 100,
        }).await
    }

    #[tokio::test]
    async fn test_create_table() {
        // Define a test configuration
        let test_conf = PgConfig {
            host: "localhost".to_string(),
            port: 5432,
            user: "postgres".to_string(),
            password: "123".to_string(),
            dbname: "sglk_sample1".to_string(),
        };
        let db_pg = init(test_conf).await;
        db_pg.create_event_table("event".to_string()).await;
    }

    #[tokio::test]
    async fn test_init_db_pg() {
        // Define a test configuration
        let test_conf = PgConfig {
            host: "localhost".to_string(),
            port: 5432,
            user: "postgres".to_string(),
            password: "123".to_string(),
            dbname: "sglk_sample1".to_string(),
        };

        // Initialize the DbPg
        let db_pg_arc = init(test_conf).await;
        let db_pg = Arc::new(db_pg_arc);
        println!("{}", db_pg.clone().get_latest_id_by_table_name("person".to_string()).await);
        let mut handles = vec![];

        // Start NUM_THREADS threads
        for _ in 0..20 {
            let db_pg_arc_clone = Arc::clone(&db_pg);
            let handle = tokio::spawn(async move {
                // Invoke query_test method on DbPg
                db_pg_arc_clone.query_test().await;
            });
            handles.push(handle);
        }

        // Join all threads
        for handle in handles {
            handle.await.expect("Thread panicked");
        }
        // Validate the initialization
        // assert_eq!(db_pg.conf.host, "localhost");
        // assert_eq!(db_pg.conf.port, 5432);
        // assert_eq!(db_pg.conf.user, "postgres");
        // assert_eq!(db_pg.conf.password, "123");
        // assert_eq!(db_pg.conf.dbname, "sglk_sample1");
        db_pg.clone().query_test().await;
    }
}
