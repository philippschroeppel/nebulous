use std::sync::Arc;
use crate::db::DbPool;
use redis::Client as RedisClient;
use rdkafka::producer::FutureProducer;
use rdkafka::admin::AdminClient;
use rdkafka::client::DefaultClientContext;

#[derive(Clone)]
pub enum MessageQueue {
    Kafka {
        producer: Arc<FutureProducer>,
        admin: Arc<AdminClient<DefaultClientContext>>,
    },
    Redis {
        client: Arc<RedisClient>,
    },
}

#[derive(Clone)]
pub struct AppState {
    pub db_pool: DbPool,
    pub message_queue: MessageQueue,
}