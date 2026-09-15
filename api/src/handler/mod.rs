pub mod node;
pub mod sync;

/// Every use case the service exposes.
#[derive(Clone)]
pub struct Handler {
    pub node: node::NodeHandler,
}
