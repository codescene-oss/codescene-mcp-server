use rmcp::model::{ClientJsonRpcMessage, ErrorCode, ErrorData, ServerJsonRpcMessage};
use rmcp::service::RoleServer;
use rmcp::transport::Transport;

const SERVER_DISCOVER_METHOD: &str = "server/discover";

pub(crate) struct DiscoveryFallbackTransport<T> {
    inner: T,
    awaiting_initialize: bool,
}

impl<T> DiscoveryFallbackTransport<T> {
    pub(crate) fn new(inner: T) -> Self {
        Self {
            inner,
            awaiting_initialize: true,
        }
    }
}

impl<T> Transport<RoleServer> for DiscoveryFallbackTransport<T>
where
    T: Transport<RoleServer>,
{
    type Error = T::Error;

    fn send(
        &mut self,
        item: ServerJsonRpcMessage,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send + 'static {
        self.inner.send(item)
    }

    async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
        loop {
            let message = self.inner.receive().await?;
            let ClientJsonRpcMessage::Request(request) = &message else {
                return Some(message);
            };
            if !self.awaiting_initialize || request.request.method() != SERVER_DISCOVER_METHOD {
                self.awaiting_initialize &= request.request.method() != "initialize";
                return Some(message);
            }

            let error = ErrorData::new(ErrorCode::METHOD_NOT_FOUND, "Method not found", None);
            if self
                .inner
                .send(ServerJsonRpcMessage::error(error, Some(request.id.clone())))
                .await
                .is_err()
            {
                return Some(message);
            }
        }
    }

    fn close(&mut self) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        self.inner.close()
    }
}
