use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use nxr_core::NxrDb;

pub struct SimpleApiServer {
    db: Arc<Mutex<NxrDb>>,
    addr: String,
}

impl SimpleApiServer {
    pub fn new(db: NxrDb, addr: &str) -> Self {
        Self {
            db: Arc::new(Mutex::new(db)),
            addr: addr.to_string(),
        }
    }

    pub fn start(&self) -> std::io::Result<()> {
        let listener = TcpListener::bind(&self.addr)?;
        log::info!("NXR API server listening on {}", self.addr);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let db = self.db.clone();
                    thread::spawn(move || {
                        handle_client(stream, db);
                    });
                }
                Err(e) => {
                    log::error!("Connection error: {}", e);
                }
            }
        }
        Ok(())
    }
}

fn handle_client(stream: TcpStream, db: Arc<Mutex<NxrDb>>) {
    let reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;

    for line in reader.lines() {
        match line {
            Ok(request) => {
                let response = process_request(&request, &db);
                let resp = format!("{}\n", response);
                let _ = writer.write_all(resp.as_bytes());
                let _ = writer.flush();
            }
            Err(_) => break,
        }
    }
}

fn process_request(request: &str, db: &Arc<Mutex<NxrDb>>) -> String {
    let parts: Vec<&str> = request.splitn(2, ' ').collect();
    match parts[0] {
        "PING" => "PONG".to_string(),
        "QUERY" => {
            let q = parts.get(1).unwrap_or(&"");
            match db.lock() {
                Ok(mut guard) => {
                    let engine = nxr_core::query::QueryEngine::new();
                    match engine.execute_with_db(q, &mut guard) {
                        Ok(result) => serde_json::to_string(&result).unwrap_or_default(),
                        Err(e) => format!("ERROR: {}", e),
                    }
                }
                Err(e) => format!("ERROR: Lock: {}", e),
            }
        }
        "VINSERT" => {
            // VINSERT id dim val1 val2 ... [metadata]
            let args: Vec<&str> = parts.get(1).unwrap_or(&"").split(' ').collect();
            if args.len() < 3 {
                return "ERROR: VINSERT id dim vals...".to_string();
            }
            let id: u64 = args[0].parse().unwrap_or(0);
            let dim: usize = args[1].parse().unwrap_or(0);
            let mut vector = Vec::with_capacity(dim);
            for i in 0..dim {
                if let Some(v) = args.get(2 + i) {
                    vector.push(v.parse::<f32>().unwrap_or(0.0));
                }
            }
            match db.lock() {
                Ok(mut db) => match db.vector.insert(id, &vector, &[]) {
                    Ok(_) => "OK".to_string(),
                    Err(e) => format!("ERROR: {}", e),
                },
                Err(e) => format!("ERROR: {}", e),
            }
        }
        "VSEARCH" => {
            let rest = parts.get(1).unwrap_or(&"");
            let vector_str: Vec<f32> = rest
                .split(',')
                .filter_map(|s| s.trim().parse::<f32>().ok())
                .collect();
            if vector_str.is_empty() {
                return "ERROR: provide vector values".to_string();
            }
            match db.lock() {
                Ok(db) => match db.vector.search(&vector_str, 10) {
                    Ok(results) => results
                        .iter()
                        .map(|(id, score)| format!("{}:{}", id, score))
                        .collect::<Vec<_>>()
                        .join(","),
                    Err(e) => format!("ERROR: {}", e),
                },
                Err(e) => format!("ERROR: {}", e),
            }
        }
        "GADD" => {
            // GADD label key1=val1 key2=val2
            let rest = parts.get(1).unwrap_or(&"");
            let mut args = rest.splitn(2, ' ');
            let label = args.next().unwrap_or("default");
            let props_str = args.next().unwrap_or("");
            let mut props = Vec::new();
            for pair in props_str.split(',') {
                if let Some((k, v)) = pair.split_once('=') {
                    props.push((k.trim().to_string(), v.trim().to_string()));
                }
            }
            match db.lock() {
                Ok(mut db) => match db.graph.add_node(label, props) {
                    Ok(id) => format!("OK:{}", id),
                    Err(e) => format!("ERROR: {}", e),
                },
                Err(e) => format!("ERROR: {}", e),
            }
        }
        "KVGET" => {
            let key = parts.get(1).unwrap_or(&"");
            match db.lock() {
                Ok(db) => match db.kv.get(key) {
                    Ok(Some(val)) => String::from_utf8_lossy(&val).to_string(),
                    Ok(None) => "NOT_FOUND".to_string(),
                    Err(e) => format!("ERROR: {}", e),
                },
                Err(e) => format!("ERROR: {}", e),
            }
        }
        "KVSET" => {
            let rest = parts.get(1).unwrap_or(&"");
            let mut args = rest.splitn(3, ' ');
            let key = args.next().unwrap_or("");
            let value = args.next().unwrap_or("");
            let ttl: u32 = args.next().unwrap_or("0").parse().unwrap_or(0);
            match db.lock() {
                Ok(mut db) => match db.kv.set(key, value.as_bytes(), ttl) {
                    Ok(_) => "OK".to_string(),
                    Err(e) => format!("ERROR: {}", e),
                },
                Err(e) => format!("ERROR: {}", e),
            }
        }
        "STATS" => {
            match db.lock() {
                Ok(db) => {
                    format!(
                        "{{\"vectors\":{},\"graph_nodes\":{},\"graph_edges\":{},\"kv_entries\":{}}}",
                        db.vector.len(),
                        db.graph.node_count(),
                        db.graph.edge_count(),
                        db.kv.len(),
                    )
                }
                Err(e) => format!("ERROR: {}", e),
            }
        }
        _ => format!("ERROR: Unknown command: {}", parts[0]),
    }
}
