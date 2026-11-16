use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};

pub mod state;
pub mod tools;

use tools::{
    db_schema::{self, DbSchemaRequest},
    dependency_graph::{self, DependencyGraphRequest},
    module_blueprint::{self, ModuleBlueprintRequest},
    tracker::{self, TrackerRequest},
};

#[derive(Debug, Clone)]
pub struct CiLegacyServer {
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl CiLegacyServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for CiLegacyServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl CiLegacyServer {
    #[tool(
        name = "build_dependency_graph",
        description = "Scan legacy CodeIgniter repository to map all modules (Controller, Model, Library, Helper) and file dependencies, providing safe migration order to Laravel."
    )]
    pub async fn build_dependency_graph(
        &self,
        Parameters(req): Parameters<DependencyGraphRequest>,
    ) -> CallToolResult {
        dependency_graph::handle_build_dependency_graph(req).await
    }

    #[tool(
        name = "extract_module_blueprint",
        description = "Compress context of a CodeIgniter module into a structured JSON blueprint (methods, inputs, sessions, models, views)."
    )]
    pub async fn extract_module_blueprint(
        &self,
        Parameters(req): Parameters<ModuleBlueprintRequest>,
    ) -> CallToolResult {
        module_blueprint::handle_extract_module_blueprint(req).await
    }

    #[tool(
        name = "inspect_database_schema",
        description = "Read database structure from SQL dump file for accurate Laravel Migration and Eloquent Model generation."
    )]
    pub async fn inspect_database_schema(
        &self,
        Parameters(req): Parameters<DbSchemaRequest>,
    ) -> CallToolResult {
        db_schema::handle_inspect_database_schema(req).await
    }

    #[tool(
        name = "track_migration_state",
        description = "Maintain cross-session migration progress and class mapping registry in .migration-state.json."
    )]
    pub async fn track_migration_state(
        &self,
        Parameters(req): Parameters<TrackerRequest>,
    ) -> CallToolResult {
        tracker::handle_track_migration_state(req).await
    }
}

#[tool_handler]
impl ServerHandler for CiLegacyServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("ci-legacy-mcp: CodeIgniter (CI3/CI4) to Laravel migration assistant MCP server")
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // IMPORTANT: Strict stdio safety - DILARANG println!() to prevent corrupting JSON-RPC streams.
    let server = CiLegacyServer::new();
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
