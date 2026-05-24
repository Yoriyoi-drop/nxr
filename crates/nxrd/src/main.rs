use clap::Parser;
use nxr_core::config::Config;
use nxr_core::NxrDb;
use nxr_api::SimpleApiServer;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "nxrd", about = "NXR AI-native database daemon")]
struct Cli {
    #[arg(short, long, default_value = "./data")]
    db_path: String,

    #[arg(short, long, default_value = "127.0.0.1:9643")]
    bind: String,

    #[arg(short, long)]
    init: bool,

    #[arg(long)]
    replay_wal: bool,

    #[arg(long, default_value = "grpc")]
    mode: String,

    #[arg(long)]
    snapshot: bool,

    #[arg(long)]
    restore: Option<String>,

    #[arg(long)]
    list_snapshots: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let cli = Cli::parse();

    if cli.init {
        initialize_db(&cli.db_path)?;
        return Ok(());
    }

    log::info!("Starting NXR database at {}", cli.db_path);

    let config_path = PathBuf::from(&cli.db_path).join("config.toml");
    let mut config = if config_path.exists() {
        Config::load(&config_path)?
    } else {
        Config::default()
    };
    config = config.with_db_path(&cli.db_path);

    let wal = Arc::new(nxr_core::wal::Wal::open(&config.wal_dir)?);

    if cli.replay_wal {
        log::info!("Replaying WAL...");
        let count = wal.replay(|_entry| Ok(()))?;
        log::info!("Replayed {} WAL entries", count);
    }

    let vector = nxr_core::vector::VectorEngine::new(&config)?
        .with_wal(Arc::clone(&wal));
    let graph = nxr_core::graph::GraphStore::new(&config)?
        .with_wal(Arc::clone(&wal));
    let kv = nxr_core::kv::KvCache::new(&config)?
        .with_wal(wal);

    let pipeline = nxr_core::pipeline::QueryPipeline::new(&config);
    let gc = pipeline.gc.clone();
    let index = nxr_core::index::IndexManager::new(&config)?;

    let snapshot_mgr = nxr_core::snapshot::SnapshotManager::new(&config)
        .with_wal(Arc::new(nxr_core::wal::Wal::open(&config.wal_dir)?));

    let db = NxrDb {
        config: config.clone(),
        wal: nxr_core::wal::Wal::open(&config.wal_dir)?,
        vector,
        graph,
        kv,
        index,
        pipeline,
        gc,
    };

    if cli.list_snapshots {
        let snapshots = snapshot_mgr.list()?;
        if snapshots.is_empty() {
            log::info!("No snapshots found");
        } else {
            log::info!("Snapshots:");
            for snap in &snapshots {
                log::info!("  {} - {} (nodes: {}, edges: {})",
                    snap.id, snap.timestamp, snap.graph_nodes, snap.graph_edges);
            }
        }
        return Ok(());
    }

    if let Some(snapshot_id) = cli.restore {
        log::info!("Restoring snapshot: {}", snapshot_id);
        let mut graph = nxr_core::graph::GraphStore::new(&config)?;
        snapshot_mgr.restore(&snapshot_id, &mut graph)?;
        log::info!("Restore complete");
        return Ok(());
    }

    db.start_gc_loop();

    if cli.snapshot {
        snapshot_mgr.create(
            &db.graph,
            &serde_json::to_vec(&db.kv.len()).unwrap_or_default(),
            &serde_json::to_vec(&db.vector.len()).unwrap_or_default(),
        )?;
        log::info!("Snapshot created on startup");
    }

    match cli.mode.as_str() {
        "grpc" => {
            let server = nxr_api::NxrGrpcServer::new(db);
            server.start(&cli.bind).await?;
        }
        "simple" | "tcp" => {
            let server = SimpleApiServer::new(db, &cli.bind);
            log::info!("NXR TCP API listening on {}", cli.bind);
            server.start()?;
        }
        _ => {
            log::error!("Unknown mode '{}'. Use 'grpc' or 'simple'", cli.mode);
            std::process::exit(1);
        }
    }

    Ok(())
}

fn initialize_db(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let db_path = PathBuf::from(path);

    let dirs = [
        "vectors/segments",
        "graph",
        "kv/cold",
        "wal",
        "indexes",
        "snapshots",
        "logs",
    ];

    for dir in &dirs {
        std::fs::create_dir_all(db_path.join(dir))?;
    }

    let config_path = db_path.join("config.toml");
    if !config_path.exists() {
        let config_content = r#"# NXR Database Configuration
[vector]
dimension = 1536
ef_construction = 200
m_max = 16
space = "cosine"
segment_size_mb = 64

[kv]
hot_zone_mb = 2048
warm_zone_mb = 51200

[pipeline]
max_context_tokens = 128000
gc_interval_hours = 24
"#;
        std::fs::write(&config_path, config_content)?;
        log::info!("Created default config at {:?}", config_path);
    }

    log::info!("Initialized NXR database at {}", path);
    Ok(())
}
