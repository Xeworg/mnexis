use rmcp::{
    ServerHandler, ServiceExt, handler::server::wrapper::Parameters, tool, tool_handler,
    tool_router, transport::stdio,
};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
struct PingArgs {
    /// Text echoed back to prove the stdio tool round trip.
    text: String,
}

#[derive(Clone)]
struct SpikeServer;

#[tool_router]
impl SpikeServer {
    #[tool(description = "Return a local MCP stdio connectivity check.")]
    fn ping(&self, Parameters(args): Parameters<PingArgs>) -> String {
        format!("pong: {}", args.text)
    }
}

#[tool_handler]
impl ServerHandler for SpikeServer {}

pub async fn serve_stdio() -> Result<(), Box<dyn std::error::Error>> {
    let service = SpikeServer.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
