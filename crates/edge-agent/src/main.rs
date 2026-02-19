use anyhow::Result;
use clap::Parser;
use dotenv::dotenv;
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use application::automation::AutomationEngine;
use application::device::DeviceManager;
use domain::device::DeviceRepository;
use domain::event::EventPublisher;
use domain::tag::TagRepository;
use infrastructure::MqttClient;
use infrastructure::config::AgentConfig;
use infrastructure::messaging::CompositeEventPublisher;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // Agent ID is now in config, but we can override it via CLI if needed,
    // though the request said "depend on configuration".
    // We will keep CLI args as overrides for config values if implemented,
    // but for now let's rely on AgentConfig.
    /// Path to config directory (optional)
    #[arg(long, default_value = "config")]
    config_dir: String,

    /// Override Agent ID
    #[arg(long)]
    agent_id: Option<String>,

    /// Override MQTT Host
    #[arg(long)]
    mqtt_host: Option<String>,

    /// Override MQTT Port
    #[arg(long)]
    mqtt_port: Option<u16>,
}

async fn run() -> Result<()> {
    dotenv().ok();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "info,edge_agent=debug,application=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("🤖 IFA SCADA Edge Agent Starting...");
    info!("🆔 Process ID: {}", std::process::id());

    // 0. Parse Args
    let args = Args::parse();

    // 0.1 Portable Directory Discovery
    // Check if we are in development environment (run from project root)
    let dev_base = "crates/edge-agent";
    let base_dir = if std::path::Path::new(dev_base).exists() {
        dev_base
    } else {
        "."
    };

    let data_dir = format!("{}/data", base_dir);
    let config_dir_path = format!("{}/config", base_dir);

    info!("📂 Base directory: {}", base_dir);
    info!("📂 Config directory: {}", config_dir_path);
    info!("📂 Data directory: {}", data_dir);

    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        eprintln!("❌ Failed to create data directory {}: {}", data_dir, e);
        return Err(e.into());
    }
    if let Err(e) = std::fs::create_dir_all(&config_dir_path) {
        eprintln!(
            "❌ Failed to create config directory {}: {}",
            config_dir_path, e
        );
        return Err(e.into());
    }

    // 1. Load Configuration
    info!("Loading configuration...");
    let mut config = AgentConfig::load(&config_dir_path)?;

    // Override with CLI args if present
    if let Some(id) = args.agent_id {
        config.agent_id = id;
    }
    if let Some(host) = args.mqtt_host {
        config.mqtt.host = host;
    }
    if let Some(port) = args.mqtt_port {
        config.mqtt.port = port;
    }

    let agent_id = config.agent_id.clone();
    info!("✅ Loaded configuration for Agent: {}", agent_id);

    // 2. Initialize MQTT
    info!(host = %config.mqtt.host, port = %config.mqtt.port, "Connecting to MQTT Broker...");

    let mqtt_client_id = format!("edge-{}", agent_id);
    let lwt_topic = format!("scada/status/{}", agent_id);

    // Last Will
    let last_will_payload = serde_json::json!({ "status": "OFFLINE" }).to_string();
    let last_will = rumqttc::LastWill::new(
        &lwt_topic,
        last_will_payload,
        rumqttc::QoS::AtLeastOnce,
        true,
    );

    let mqtt_client = MqttClient::new(
        &config.mqtt.host,
        config.mqtt.port,
        &mqtt_client_id,
        Some(last_will),
    )
    .await?;

    info!("✅ Connected to MQTT Broker");

    // 3. Initialize Database & Repository
    let db_path = format!("sqlite://{}/{}_storage.db?mode=rwc", data_dir, agent_id);
    info!("💾 Connecting to Storage: {}", db_path);

    let db = sea_orm::Database::connect(&db_path).await?;
    // 3.1 Ensure Schema Exists
    {
        use infrastructure::database::entities::{edge_agents, tags};
        use sea_orm::{
            ActiveModelTrait, ConnectionTrait, DbBackend, EntityTrait, Schema, Set, Statement,
        };

        let backend = DbBackend::Sqlite;
        let schema = Schema::new(backend);

        // 1. Create edge_agents table (Reference for devices)
        let stmt_agent = schema
            .create_table_from_entity(edge_agents::Entity)
            .if_not_exists()
            .to_owned();
        let sql_agent = stmt_agent.build(sea_orm::sea_query::SqliteQueryBuilder);
        db.execute(Statement::from_string(backend, sql_agent.to_string()))
            .await?;

        // 2. Create devices table (FK to edge_agents, referenced by tags)
        let stmt_devices = schema
            .create_table_from_entity(infrastructure::database::entities::devices::Entity)
            .if_not_exists()
            .to_owned();
        let sql_devices = stmt_devices.build(sea_orm::sea_query::SqliteQueryBuilder);
        db.execute(Statement::from_string(backend, sql_devices.to_string()))
            .await?;

        // 3. Create tags table (FK to devices)
        let stmt_tags = schema
            .create_table_from_entity(tags::Entity)
            .if_not_exists()
            .to_owned();
        let sql_tags = stmt_tags.build(sea_orm::sea_query::SqliteQueryBuilder);
        db.execute(Statement::from_string(backend, sql_tags.to_string()))
            .await?;

        // 3. Create reports table
        let stmt_reports = schema
            .create_table_from_entity(infrastructure::database::entities::reports::Entity)
            .if_not_exists()
            .to_owned();
        let sql_reports = stmt_reports.build(sea_orm::sea_query::SqliteQueryBuilder);
        db.execute(Statement::from_string(backend, sql_reports.to_string()))
            .await?;

        info!("✅ Schema verified (tables created)");

        // 4. Ensure current agent exists
        let agent_exists = edge_agents::Entity::find_by_id(agent_id.clone())
            .one(&db)
            .await?;
        if agent_exists.is_none() {
            info!(
                "Run-time initialization: Creating default agent record for {}",
                agent_id
            );
            // Use current time
            let now = chrono::Utc::now().fixed_offset();
            let new_agent = edge_agents::ActiveModel {
                id: Set(agent_id.clone()),
                description: Set(Some("Local Edge Agent".to_string())),
                status: Set(Some("online".to_string())),
                created_at: Set(Some(now)),
                updated_at: Set(Some(now)),
                ..Default::default()
            };
            new_agent.insert(&db).await?;
        }
    }

    let tag_repository = Arc::new(infrastructure::SeaOrmTagRepository::new(db.clone()));
    let device_repository = Arc::new(infrastructure::SeaOrmDeviceRepository::new(
        db.clone(),
        agent_id.clone(),
    ));

    // 4. Initialize Services (Buffered MQTT Publisher)
    let buffer_path = format!("sqlite://{}/{}_buffer.db?mode=rwc", data_dir, agent_id);

    let sqlite_buffer = infrastructure::database::SQLiteBuffer::new(&buffer_path).await?;
    info!(
        "💾 Initialized SQLite Buffer (Store & Forward) at {}",
        buffer_path
    );

    let client_arc: Arc<dyn infrastructure::messaging::mqtt_client::MqttPublisherClient> =
        Arc::new(mqtt_client.clone());

    let mqtt_publisher = Arc::new(infrastructure::BufferedMqttPublisher::new(
        client_arc,
        sqlite_buffer,
        agent_id.clone(),
    ));

    // Initialize Printer Manager & Executor
    let action_executor: Arc<dyn application::automation::executor::ActionExecutor> = if let Some(
        printer_config,
    ) =
        &config.printer
    {
        if printer_config.enabled {
            info!(host=%printer_config.host, port=%printer_config.port, "🖨️ Printer Enabled");
            let (print_tx, print_rx) = tokio::sync::mpsc::channel(32);

            let printer: Box<dyn domain::printer::PrinterConnection> = if printer_config
                .r#type
                .as_deref()
                == Some("File")
                || printer_config.path.is_some()
            {
                let path = printer_config
                    .path
                    .as_deref()
                    .unwrap_or("printer_output.txt");
                info!(path=%path, "🖨️ Initializing File/Share Printer");
                Box::new(infrastructure::printer::FilePrinter::new(path))
                    as Box<dyn domain::printer::PrinterConnection>
            } else {
                info!(host=%printer_config.host, port=%printer_config.port, "🖨️ Initializing Network Printer");
                Box::new(infrastructure::printer::NetworkPrinter::new(
                    &printer_config.host,
                    printer_config.port,
                )) as Box<dyn domain::printer::PrinterConnection>
            };

            let manager = application::printer::manager::PrinterManager::new(printer, print_rx);
            tokio::spawn(manager.run());
            Arc::new(
                application::automation::executor::PrintingActionExecutor::new(
                    print_tx,
                    agent_id.clone(),
                    mqtt_publisher.clone(),
                ),
            )
        } else {
            Arc::new(application::automation::executor::LoggingActionExecutor)
        }
    } else {
        Arc::new(application::automation::executor::LoggingActionExecutor)
    };

    // Initialize Automation Engine
    let automation_engine = Arc::new(AutomationEngine::new(
        config.tags.clone(),
        action_executor.clone(),
    ));

    // Import Devices FIRST (tags have FK → devices, must exist before tags)
    let existing_devices = device_repository.find_by_agent(&agent_id).await?;
    if existing_devices.is_empty() && !config.devices.is_empty() {
        info!(
            "📥 Importing {} devices from initial config to DB...",
            config.devices.len()
        );
        for device in &config.devices {
            device_repository.save(device).await?;
        }
        info!("✅ Device Import complete");
    }

    // Import Tags second (after devices exist)
    let existing_tags = tag_repository.find_by_agent(&agent_id).await?;
    if existing_tags.is_empty() && !config.tags.is_empty() {
        info!(
            "📥 Importing {} tags from initial config to DB...",
            config.tags.len()
        );
        let temp_repo =
            infrastructure::repositories::ConfigTagRepository::new(&agent_id, config.tags.clone());
        let initial_tags = temp_repo.find_all().await?;
        for tag in initial_tags {
            if let Err(e) = tag_repository.save(&tag).await {
                warn!(
                    "Failed to import initial tag {}: {}. Skipping.",
                    tag.id(),
                    e
                );
            }
        }
        info!("✅ Tag Import complete");
    }

    // Create Composite Publisher (MQTT + Automation)
    let composite_publisher = Arc::new(CompositeEventPublisher::new(vec![
        mqtt_publisher.clone(),
        automation_engine.clone(),
    ]));

    // Device Manager (replaces ExecutorManager)
    let device_manager = Arc::new(DeviceManager::new(composite_publisher.clone()));

    // 5. Load Tags & Devices from Repo (Persistent Source)
    let tags = tag_repository.find_by_agent(&agent_id).await?;
    let devices = device_repository.find_by_agent(&agent_id).await?;
    info!(
        "📋 Loaded {} tag(s) and {} device(s) from storage",
        tags.len(),
        devices.len()
    );

    // 6. Start Executors (Devices)
    device_manager.start_devices(devices, tags).await;

    // 7. Start Command Listener
    let command_listener = application::CommandListener::new(
        mqtt_client.clone(),
        agent_id.clone(),
        action_executor.clone(),
    );
    let listener_agent_id = agent_id.clone();
    tokio::spawn(async move {
        info!(agent_id = %listener_agent_id, "Starting Command Listener");
        command_listener.start().await;
    });

    // 7.5 Start Config Manager (Remote Configuration)
    // use application::device::DeviceManager; // Already imported at top

    let config_path = std::path::PathBuf::from(format!("{}/last_known.json", config_dir_path));

    // Shared Config Version for Heartbeat
    let config_version = Arc::new(std::sync::RwLock::new(config.version.clone()));

    let config_manager = edge_agent::config_manager::ConfigManager::new(
        mqtt_client.clone(),
        config_path,
        agent_id.clone(),
        device_manager.clone(), // NEW
        // executor_manager.clone(), // Removed
        automation_engine.clone(),
        tag_repository.clone(),
        device_repository.clone(), // Added
        config_version.clone(),
    );

    // Ensure we subscribe BEFORE coming ONLINE
    // We must capture the receiver here to avoid race conditions with retained messages
    let config_rx = match config_manager.init().await {
        Ok(rx) => Some(rx),
        Err(e) => {
            warn!(
                "Failed to initialize ConfigManager subscription: {}. Retrying later...",
                e
            );
            None
        }
    };

    tokio::spawn(async move {
        if let Some(rx) = config_rx {
            config_manager.run_loop(rx).await;
        } else {
            // Fallback: try to get a new receiver if init failed (though subscription likely failed too)
            warn!(
                "ConfigManager started without initial receiver. Attempting to subscribe to internal channel anyway."
            );
        }
    });

    // 8. Publish ONLINE status (After ConfigManager is listening)
    info!("✅ Agent Initialized. Publishing ONLINE status...");
    let online_payload = serde_json::json!({
        "status": "ONLINE",
        "version": *config_version.read().unwrap()
    })
    .to_string();

    if let Err(e) = mqtt_client.publish(&lwt_topic, &online_payload, true).await {
        warn!("Failed to publish ONLINE status: {}", e);
    }

    // 9. Heartbeat Loop
    let heartbeat_agent_id = agent_id.clone();
    let manager_arc = device_manager.clone();
    let heartbeat_manager = manager_arc.clone();
    let heartbeat_publisher = mqtt_publisher.clone();
    let heartbeat_version_lock = config_version.clone();

    let heartbeat_interval = config.heartbeat_interval_secs;
    let heartbeat_handle = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(heartbeat_interval));
        let start_time = std::time::Instant::now();

        loop {
            interval.tick().await;
            let uptime = start_time.elapsed().as_secs();
            let active_tag_ids = heartbeat_manager.get_active_tag_ids().await;

            let current_version = heartbeat_version_lock.read().unwrap().clone();

            let event = domain::event::DomainEvent::agent_heartbeat(
                &heartbeat_agent_id,
                &current_version,
                uptime,
                active_tag_ids,
            );

            if let Err(e) = heartbeat_publisher.publish(event).await {
                warn!(error = %e, "Failed to publish heartbeat");
            } else {
                info!("💓 Heartbeat sent (v{})", current_version);
            }
        }
    });

    // 9. Shutdown Signal
    match tokio::signal::ctrl_c().await {
        Ok(()) => info!("🛑 Shutting down..."),
        Err(err) => warn!(error = %err, "Unable to listen for shutdown signal"),
    }

    manager_arc.stop_all().await;
    heartbeat_handle.abort();

    // Publish OFFLINE before exit (Best effort)
    let offline_payload = serde_json::json!({ "status": "OFFLINE" }).to_string();
    let _ = mqtt_client
        .publish(&lwt_topic, &offline_payload, true)
        .await;

    info!("👋 Good bye!");
    Ok(())
}

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    if let Err(e) = rt.block_on(run()) {
        eprintln!("\n❌ CRITICAL ERROR: {:?}", e);
        eprintln!("--------------------------------------------------");
        eprintln!("La aplicación se cerró debido a un error fatal.");

        #[cfg(target_os = "windows")]
        {
            eprintln!("\nPresiona Enter para cerrar esta ventana...");
            let mut input = String::new();
            let _ = std::io::stdin().read_line(&mut input);
        }

        std::process::exit(1);
    }
}
