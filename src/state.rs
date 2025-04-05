use crate::db::DbPool;
use rdkafka::admin::AdminClient;
use rdkafka::client::DefaultClientContext;
use rdkafka::producer::FutureProducer;
use redis::Client as RedisClient;
use std::sync::Arc;

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
