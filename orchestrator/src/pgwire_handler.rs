use crate::query_router;
use anyhow::Result;
use async_trait::async_trait;
use futures::Sink;
use pgwire::api::auth::noop::NoopStartupHandler;
use pgwire::api::copy::NoopCopyHandler;
use pgwire::api::query::{PlaceholderExtendedQueryHandler, SimpleQueryHandler};
use pgwire::api::results::Response;
use pgwire::api::{ClientInfo, NoopErrorHandler, PgWireServerHandlers};
use pgwire::error::{PgWireError, PgWireResult};
use pgwire::messages::PgWireBackendMessage;
use pgwire::tokio::process_socket;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{error, info};

pub struct SnowdropStartupHandler;

impl NoopStartupHandler for SnowdropStartupHandler {}

pub struct SnowdropQueryHandler {
    router: Arc<query_router::QueryRouter>,
}

#[async_trait]
impl SimpleQueryHandler for SnowdropQueryHandler {
    async fn do_query<'a, 'b: 'a, C>(
        &'b self,
        _client: &mut C,
        query_str: &'a str,
    ) -> PgWireResult<Vec<Response<'a>>>
    where
        C: ClientInfo + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        info!(query = query_str, "received query");
        self.router.route(query_str).await
    }
}

struct SnowdropHandlers {
    startup: Arc<SnowdropStartupHandler>,
    query: Arc<SnowdropQueryHandler>,
    extended: Arc<PlaceholderExtendedQueryHandler>,
    copy: Arc<NoopCopyHandler>,
    error: Arc<NoopErrorHandler>,
}

impl PgWireServerHandlers for SnowdropHandlers {
    type StartupHandler = SnowdropStartupHandler;
    type SimpleQueryHandler = SnowdropQueryHandler;
    type ExtendedQueryHandler = PlaceholderExtendedQueryHandler;
    type CopyHandler = NoopCopyHandler;
    type ErrorHandler = NoopErrorHandler;

    fn simple_query_handler(&self) -> Arc<Self::SimpleQueryHandler> {
        self.query.clone()
    }

    fn extended_query_handler(&self) -> Arc<Self::ExtendedQueryHandler> {
        self.extended.clone()
    }

    fn startup_handler(&self) -> Arc<Self::StartupHandler> {
        self.startup.clone()
    }

    fn copy_handler(&self) -> Arc<Self::CopyHandler> {
        self.copy.clone()
    }

    fn error_handler(&self) -> Arc<Self::ErrorHandler> {
        self.error.clone()
    }
}

pub async fn serve_pgwire(
    addr: &str,
    neo4j: schema::Neo4jConnection,
    kvrocks: executor::KvrocksConnection,
    wal: wal::WalFile,
) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(addr, "pgwire listener started");

    let router = Arc::new(query_router::QueryRouter::new(neo4j, kvrocks, wal));

    let handlers = Arc::new(SnowdropHandlers {
        startup: Arc::new(SnowdropStartupHandler),
        query: Arc::new(SnowdropQueryHandler {
            router: router.clone(),
        }),
        extended: Arc::new(PlaceholderExtendedQueryHandler),
        copy: Arc::new(NoopCopyHandler),
        error: Arc::new(NoopErrorHandler),
    });

    loop {
        let (socket, peer_addr) = listener.accept().await?;
        info!(%peer_addr, "client connected");

        let h = handlers.clone();
        tokio::spawn(async move {
            if let Err(e) = process_socket(socket, None, h).await {
                error!(%peer_addr, "connection error: {e}");
            }
            info!(%peer_addr, "client disconnected");
        });
    }
}
