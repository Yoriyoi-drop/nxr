pub mod server;
pub mod simple_server;

pub use simple_server::SimpleApiServer;
pub use server::NxrGrpcServer;

pub mod pb {
    tonic::include_proto!("nxr");
}
