pub mod api;
pub mod services;
pub mod state;

use infrastructure::MqttClient;
use sqlx::PgPool;
use state::AppState;
use std::sync::Arc;

pub async fn setup_app_state(
    pool: PgPool,
    mqtt_client: MqttClient,
    buffer: infrastructure::database::SQLiteBuffer,
) -> Arc<AppState> {
    Arc::new(AppState::new(mqtt_client, pool, buffer))
}
