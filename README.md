# NXR — Vector + Graph Database

NXR adalah database embedded yang menggabungkan vector search, graph traversal, dan key-value store dalam satu engine, ditulis dalam Rust.

## Fitur

- **Vector Search** — HNSW index dengan cosine/euclidean distance, persistence binary, segment management
- **Graph Store** — Node/edge dengan label, adjacency list, traversal multi-hop
- **KV Cache** — Hot/warm tiering, TTL, disk spill
- **NXR-QL** — Query language untuk match/insert/delete dengan pattern matching
- **WAL** — Write-ahead log dengan checksum, replay, segment merge
- **Index** — B+ Tree + Inverted index + Garbage Collection (auto-compaction, rebuild)
- **Pipeline** — Context manager, memory ranker (recency/importance), embedding
- **Snapshot** — Create, list, restore point-in-time

## Arsitektur

```
crates/
├── nxr-core/        # Database engine inti
├── nxr-ql/          # Query language parser (NXR-QL)
├── nxr-api/         # gRPC + Simple TCP server
├── nxr-sdk/         # Rust SDK (thread-safe client)
├── nxr-js/          # Node.js bindings (napi-rs)
└── nxrd/            # Daemon binary

sdk/python/          # Python SDK (TCP client)
tools/nxr-cli/       # Go CLI
```

## Quick Start

### Rust (SDK)

```rust
use nxr_sdk::NxrClient;

let client = NxrClient::open("/tmp/nxr-db")?;

// Vector
client.vector_insert(1, &[0.1, 0.2, 0.3, 0.4], b"metadata")?;
let results = client.vector_search(&[0.1, 0.2, 0.3, 0.4], 10)?;

// Graph
let alice = client.graph_add_node("User", vec![("name".into(), "Alice".into())])?;
let topic = client.graph_add_node("Topic", vec![("name".into(), "Rust".into())])?;
client.graph_add_edge(alice, topic, "PREFERS", 0.9)?;

// KV
client.kv_set("greeting", b"hello", 0)?;
let val = client.kv_get("greeting")?;

// Query
let result = client.query("MATCH (u:User)-[p:PREFERS]->(t:Topic) RETURN u, p, t")?;
```

### Node.js

```js
const { NxrDatabase } = require('nxr-js');
const db = new NxrDatabase('/tmp/nxr-db');

db.vectorInsert(1, [0.1, 0.2, 0.3, 0.4]);
const results = db.vectorSearch([0.1, 0.2, 0.3, 0.4], 10);
```

### Python

```python
from nxr import NxrClient

with NxrClient("127.0.0.1", 9643) as client:
    client.vector.insert(1, [0.1, 0.2, 0.3, 0.4])
    results = client.vector.search([0.1, 0.2, 0.3, 0.4])
    node_id = client.graph.add_node("User", {"name": "Alice"})
```

### CLI

```bash
nxrd --db-path /tmp/nxr-db --mode simple
# Query via TCP:
echo "QUERY MATCH (u:User) RETURN u" | nc localhost 9643
```

## Build

```bash
# Build semua crate
cargo build --release

# Python SDK
cd sdk/python && pip install -e .

# Go CLI
cd tools/nxr-cli && go build -o nxr
```

## License

MIT
