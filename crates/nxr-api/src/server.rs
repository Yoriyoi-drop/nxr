use crate::pb;
use crate::pb::nxr_service_server::NxrService;
use nxr_core::error::NxrError;
use nxr_core::NxrDb;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

pub struct NxrGrpcServer {
    pub db: Arc<RwLock<NxrDb>>,
    start_time: Instant,
}

impl NxrGrpcServer {
    pub fn new(db: NxrDb) -> Self {
        Self {
            db: Arc::new(RwLock::new(db)),
            start_time: Instant::now(),
        }
    }

    pub async fn start(self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        use tonic::transport::Server;
        let svc = pb::nxr_service_server::NxrServiceServer::new(self);
        log::info!("NXR gRPC server listening on {}", addr);
        Server::builder()
            .add_service(svc)
            .serve(addr.parse()?)
            .await?;
        Ok(())
    }

    fn map_err(e: NxrError) -> Status {
        Status::internal(e.to_string())
    }
}

#[tonic::async_trait]
impl NxrService for NxrGrpcServer {
    async fn query(
        &self,
        request: Request<pb::QueryRequest>,
    ) -> Result<Response<pb::QueryResponse>, Status> {
        let req = request.into_inner();
        let mut db = self.db.write().await;
        let engine = nxr_core::query::QueryEngine::new();
        let result = engine
            .execute_with_db(&req.query, &mut db)
            .map_err(Self::map_err)?;

        let columns: Vec<pb::Column> = result
            .columns
            .iter()
            .map(|c| pb::Column {
                name: c.clone(),
                r#type: "string".into(),
            })
            .collect();

        let rows: Vec<pb::Row> = result
            .rows
            .iter()
            .map(|r| pb::Row {
                values: r
                    .iter()
                    .map(|v| serde_json::to_vec(v).unwrap_or_default())
                    .collect(),
            })
            .collect();

        Ok(Response::new(pb::QueryResponse {
            columns,
            rows,
            elapsed_ms: result.elapsed_ms,
        }))
    }

    async fn vector_insert(
        &self,
        request: Request<pb::VectorInsertRequest>,
    ) -> Result<Response<pb::VectorInsertResponse>, Status> {
        let req = request.into_inner();
        let mut db = self.db.write().await;
        db.vector
            .insert(req.id, &req.vector, &req.metadata)
            .map_err(Self::map_err)?;
        Ok(Response::new(pb::VectorInsertResponse { success: true }))
    }

    async fn vector_search(
        &self,
        request: Request<pb::VectorSearchRequest>,
    ) -> Result<Response<pb::VectorSearchResponse>, Status> {
        let req = request.into_inner();
        let db = self.db.read().await;
        let results = db
            .vector
            .search(&req.query, req.k as usize)
            .map_err(Self::map_err)?;
        let results: Vec<pb::VectorResult> = results
            .into_iter()
            .map(|(id, score)| pb::VectorResult { id, score })
            .collect();
        Ok(Response::new(pb::VectorSearchResponse { results }))
    }

    async fn graph_add_node(
        &self,
        request: Request<pb::GraphNodeRequest>,
    ) -> Result<Response<pb::GraphNodeResponse>, Status> {
        let req = request.into_inner();
        let mut db = self.db.write().await;
        let props: Vec<(String, String)> = req.properties.into_iter().collect();
        let id = db
            .graph
            .add_node(&req.label, props)
            .map_err(Self::map_err)?;
        Ok(Response::new(pb::GraphNodeResponse { id }))
    }

    async fn graph_add_edge(
        &self,
        request: Request<pb::GraphEdgeRequest>,
    ) -> Result<Response<pb::GraphEdgeResponse>, Status> {
        let req = request.into_inner();
        let mut db = self.db.write().await;
        let id = db
            .graph
            .add_edge(req.from_node, req.to_node, &req.relation, req.weight)
            .map_err(Self::map_err)?;
        Ok(Response::new(pb::GraphEdgeResponse { id }))
    }

    async fn graph_traverse(
        &self,
        request: Request<pb::GraphTraverseRequest>,
    ) -> Result<Response<pb::GraphTraverseResponse>, Status> {
        let req = request.into_inner();
        let db = self.db.read().await;
        let results = db.graph.traverse(&req.from_label, &req.relation, &req.to_label);
        let results: Vec<pb::TraverseResult> = results
            .into_iter()
            .map(|(from_id, to_id, weight)| pb::TraverseResult {
                from_id,
                to_id,
                weight,
            })
            .collect();
        Ok(Response::new(pb::GraphTraverseResponse { results }))
    }

    async fn kv_set(
        &self,
        request: Request<pb::KvSetRequest>,
    ) -> Result<Response<pb::KvSetResponse>, Status> {
        let req = request.into_inner();
        let mut db = self.db.write().await;
        db.kv
            .set(&req.key, &req.value, req.ttl)
            .map_err(Self::map_err)?;
        Ok(Response::new(pb::KvSetResponse { success: true }))
    }

    async fn kv_get(
        &self,
        request: Request<pb::KvGetRequest>,
    ) -> Result<Response<pb::KvGetResponse>, Status> {
        let req = request.into_inner();
        let db = self.db.read().await;
        match db.kv.get(&req.key).map_err(Self::map_err)? {
            Some(value) => Ok(Response::new(pb::KvGetResponse {
                value,
                found: true,
            })),
            None => Ok(Response::new(pb::KvGetResponse {
                value: Vec::new(),
                found: false,
            })),
        }
    }

    async fn health(
        &self,
        _request: Request<pb::HealthRequest>,
    ) -> Result<Response<pb::HealthResponse>, Status> {
        let db = self.db.read().await;
        Ok(Response::new(pb::HealthResponse {
            status: "ok".into(),
            uptime_secs: self.start_time.elapsed().as_secs(),
            vector_count: db.vector.len() as u64,
            graph_nodes: db.graph.node_count() as u64,
            graph_edges: db.graph.edge_count() as u64,
            kv_entries: db.kv.len() as u64,
        }))
    }
}
