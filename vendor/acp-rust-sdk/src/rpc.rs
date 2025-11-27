// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    any::Any,
    collections::HashMap,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
};

use agent_client_protocol_schema::Error;
use anyhow::Result;
use futures::{
    AsyncBufReadExt as _, AsyncRead, AsyncWrite, AsyncWriteExt as _, FutureExt as _,
    StreamExt as _,
    channel::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    future::LocalBoxFuture,
    io::BufReader,
    select_biased,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use serde_json::value::RawValue;

use super::stream_broadcast::{StreamBroadcast, StreamReceiver, StreamSender};

/// Stateless dispatcher that converts JSON-RPC messages into typed requests/notifications
/// and produces responses as `serde_json::Value`. This allows embedders to supply their own
/// transport/framing (e.g., WebSocket text frames) while reusing the SDK's decoding/dispatch
/// logic.
pub struct RpcDispatcher<Local: Side, Remote: Side> {
    _marker: std::marker::PhantomData<(Local, Remote)>,
}

impl<Local: Side, Remote: Side> Default for RpcDispatcher<Local, Remote> {
    fn default() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Local: Side, Remote: Side> RpcDispatcher<Local, Remote> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Handle a parsed JSON-RPC message. Returns an optional response payload to send.
    pub(crate) async fn handle_value(
        &mut self,
        value: Value,
        incoming_tx: &UnboundedSender<IncomingMessage<Local>>,
        pending_responses: &Arc<Mutex<HashMap<Id, PendingResponse>>>,
        broadcast: &StreamSender,
    ) -> Option<Value> {
        match serde_json::from_value::<RawIncomingOwned>(value) {
            Ok(message) => {
                let params_raw = message
                    .params
                    .as_ref()
                    .and_then(|v| serde_json::to_string(v).ok())
                    .and_then(|s| RawValue::from_string(s).ok());
                let result_raw = message
                    .result
                    .as_ref()
                    .and_then(|v| serde_json::to_string(v).ok())
                    .and_then(|s| RawValue::from_string(s).ok());
                let borrowed = RawIncomingMessage {
                    id: message.id,
                    method: message.method.as_deref(),
                    params: params_raw.as_deref(),
                    result: result_raw.as_deref(),
                    error: message.error,
                };
                self.handle_raw(borrowed, incoming_tx, pending_responses, broadcast).await
            }
            Err(err) => {
                log::debug!("Error decoding message: {}", err);
                None
            }
        }
    }

    async fn handle_raw<'a>(
        &mut self,
        message: RawIncomingMessage<'a>,
        incoming_tx: &UnboundedSender<IncomingMessage<Local>>,
        pending_responses: &Arc<Mutex<HashMap<Id, PendingResponse>>>,
        broadcast: &StreamSender,
    ) -> Option<Value> {
        if let Some(id) = message.id.clone() {
            if let Some(method) = message.method.clone() {
                // Request
                match Local::decode_request(method, message.params) {
                    Ok(request) => {
                        broadcast.incoming_request(id.clone(), method, &request);
                        let _ =
                            incoming_tx.unbounded_send(IncomingMessage::Request { id, request });
                    }
                    Err(err) => {
                        broadcast.incoming_response(id.clone(), Err(&err));
                        let response = OutgoingMessage::<Local, Remote>::Response {
                            id,
                            result: ResponseResult::Error(err),
                        };
                        return Some(serde_json::to_value(JsonRpcMessage::wrap(&response)).ok()?);
                    }
                }
            } else if let Some(pending_response) = pending_responses.lock().remove(&id) {
                // Response
                if let Some(result_value) = message.result {
                    broadcast.incoming_response(id.clone(), Ok(Some(result_value)));
                    let result = (pending_response.deserialize)(result_value);
                    pending_response.respond.send(result).ok();
                } else if let Some(error) = message.error {
                    broadcast.incoming_response(id.clone(), Err(&error));
                    pending_response.respond.send(Err(error)).ok();
                } else {
                    broadcast.incoming_response(id.clone(), Ok(None));
                    let result = (pending_response.deserialize)(
                        &RawValue::from_string("null".into()).unwrap(),
                    );
                    pending_response.respond.send(result).ok();
                }
            } else {
                log::error!("received response for unknown request id: {id:?}");
            }
        } else if let Some(method) = message.method {
            // Notification
            match Local::decode_notification(method, message.params) {
                Ok(notification) => {
                    broadcast.incoming_notification(method, &notification);
                    let _ =
                        incoming_tx.unbounded_send(IncomingMessage::Notification { notification });
                }
                Err(err) => log::debug!("Error decoding notification: {}", err),
            }
        }
        None
    }

    /// Format an outgoing message (response/notification) as JSON value.
    pub fn outgoing_to_value(
        &self,
        message: &OutgoingMessage<Local, Remote>,
    ) -> Result<Value, Error> {
        serde_json::to_value(JsonRpcMessage::wrap(message)).map_err(Error::into_internal_error)
    }
}

pub struct RpcConnection<Local: Side, Remote: Side> {
    outgoing_tx: UnboundedSender<OutgoingMessage<Local, Remote>>,
    pending_responses: Arc<Mutex<HashMap<Id, PendingResponse>>>,
    next_id: AtomicI64,
    broadcast: StreamBroadcast,
    dispatcher: Arc<Mutex<RpcDispatcher<Local, Remote>>>,
}

pub(crate) struct PendingResponse {
    deserialize: fn(&serde_json::value::RawValue) -> Result<Box<dyn Any + Send>, Error>,
    respond: oneshot::Sender<Result<Box<dyn Any + Send>, Error>>,
}

impl<Local, Remote> RpcConnection<Local, Remote>
where
    Local: Side + 'static,
    Remote: Side + 'static,
{
    pub fn new<Handler>(
        handler: Handler,
        outgoing_bytes: impl Unpin + AsyncWrite,
        incoming_bytes: impl Unpin + AsyncRead,
        spawn: impl Fn(LocalBoxFuture<'static, ()>) + 'static,
    ) -> (Self, impl futures::Future<Output = Result<()>>)
    where
        Handler: MessageHandler<Local> + 'static,
    {
        let (incoming_tx, incoming_rx) = mpsc::unbounded();
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded();

        let pending_responses = Arc::new(Mutex::new(HashMap::default()));
        let (broadcast_tx, broadcast) = StreamBroadcast::new();
        let dispatcher = Arc::new(Mutex::new(RpcDispatcher::new()));

        let io_task = {
            let pending_responses = pending_responses.clone();
            let dispatcher = dispatcher.clone();
            async move {
                let result = Self::handle_io(
                    incoming_tx,
                    outgoing_rx,
                    outgoing_bytes,
                    incoming_bytes,
                    pending_responses.clone(),
                    broadcast_tx,
                    dispatcher,
                )
                .await;
                pending_responses.lock().clear();
                result
            }
        };

        Self::handle_incoming(outgoing_tx.clone(), incoming_rx, handler, spawn);

        let this = Self {
            outgoing_tx,
            pending_responses,
            next_id: AtomicI64::new(0),
            broadcast,
            dispatcher,
        };

        (this, io_task)
    }

    pub fn subscribe(&self) -> StreamReceiver {
        self.broadcast.receiver()
    }

    pub fn notify(
        &self,
        method: impl Into<Arc<str>>,
        params: Option<Remote::InNotification>,
    ) -> Result<(), Error> {
        self.outgoing_tx
            .unbounded_send(OutgoingMessage::Notification {
                method: method.into(),
                params,
            })
            .map_err(|_| Error::internal_error().with_data("failed to send notification"))
    }

    pub fn request<Out: DeserializeOwned + Send + 'static>(
        &self,
        method: impl Into<Arc<str>>,
        params: Option<Remote::InRequest>,
    ) -> impl Future<Output = Result<Out, Error>> {
        let (tx, rx) = oneshot::channel();
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let id = Id::Number(id);
        self.pending_responses.lock().insert(
            id.clone(),
            PendingResponse {
                deserialize: |value| {
                    serde_json::from_str::<Out>(value.get()).map(|out| Box::new(out) as _).map_err(
                        |_| Error::internal_error().with_data("failed to deserialize response"),
                    )
                },
                respond: tx,
            },
        );

        if self
            .outgoing_tx
            .unbounded_send(OutgoingMessage::Request {
                id: id.clone(),
                method: method.into(),
                params,
            })
            .is_err()
        {
            self.pending_responses.lock().remove(&id);
        }
        async move {
            let result = rx
                .await
                .map_err(|_| Error::internal_error().with_data("server shut down unexpectedly"))??
                .downcast::<Out>()
                .map_err(|_| Error::internal_error().with_data("failed to deserialize response"))?;

            Ok(*result)
        }
    }

    async fn handle_io(
        incoming_tx: UnboundedSender<IncomingMessage<Local>>,
        mut outgoing_rx: UnboundedReceiver<OutgoingMessage<Local, Remote>>,
        mut outgoing_bytes: impl Unpin + AsyncWrite,
        incoming_bytes: impl Unpin + AsyncRead,
        pending_responses: Arc<Mutex<HashMap<Id, PendingResponse>>>,
        broadcast: StreamSender,
        dispatcher: Arc<Mutex<RpcDispatcher<Local, Remote>>>,
    ) -> Result<()> {
        // TODO: Create nicer abstraction for broadcast
        let mut input_reader = BufReader::new(incoming_bytes);
        let mut outgoing_line = Vec::new();
        let mut incoming_line = String::new();
        loop {
            select_biased! {
                message = outgoing_rx.next() => {
                    if let Some(message) = message {
                        outgoing_line.clear();
                        serde_json::to_writer(&mut outgoing_line, &JsonRpcMessage::wrap(&message)).map_err(Error::into_internal_error)?;
                        log::trace!("send: {}", String::from_utf8_lossy(&outgoing_line));
                        outgoing_line.push(b'\n');
                        outgoing_bytes.write_all(&outgoing_line).await.ok();
                        broadcast.outgoing(&message);
                    } else {
                        break;
                    }
                }
                bytes_read = input_reader.read_line(&mut incoming_line).fuse() => {
                    if bytes_read.map_err(Error::into_internal_error)? == 0 {
                        break
                    }
                    log::trace!("recv: {}", &incoming_line);

                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&incoming_line) {
                        if let Some(response) = dispatcher.lock().handle_value(value, &incoming_tx, &pending_responses, &broadcast).await {
                            outgoing_line.clear();
                            serde_json::to_writer(&mut outgoing_line, &response).map_err(Error::into_internal_error)?;
                            log::trace!("send: {}", String::from_utf8_lossy(&outgoing_line));
                            outgoing_line.push(b'\n');
                            outgoing_bytes.write_all(&outgoing_line).await.ok();
                            broadcast.outgoing_json(&response);
                        }
                    } else {
                        log::debug!("failed to parse incoming message: {}", incoming_line.trim());
                    }
                    incoming_line.clear();
                }
            }
        }
        Ok(())
    }

    fn handle_incoming<Handler: MessageHandler<Local> + 'static>(
        outgoing_tx: UnboundedSender<OutgoingMessage<Local, Remote>>,
        mut incoming_rx: UnboundedReceiver<IncomingMessage<Local>>,
        handler: Handler,
        spawn: impl Fn(LocalBoxFuture<'static, ()>) + 'static,
    ) {
        let spawn = Rc::new(spawn);
        let handler = Rc::new(handler);
        spawn({
            let spawn = spawn.clone();
            async move {
                while let Some(message) = incoming_rx.next().await {
                    match message {
                        IncomingMessage::Request { id, request } => {
                            let outgoing_tx = outgoing_tx.clone();
                            let handler = handler.clone();
                            spawn(
                                async move {
                                    let result = handler.handle_request(request).await.into();
                                    outgoing_tx
                                        .unbounded_send(OutgoingMessage::Response { id, result })
                                        .ok();
                                }
                                .boxed_local(),
                            );
                        }
                        IncomingMessage::Notification { notification } => {
                            let handler = handler.clone();
                            spawn(
                                async move {
                                    if let Err(err) =
                                        handler.handle_notification(notification).await
                                    {
                                        log::error!("failed to handle notification: {err:?}");
                                    }
                                }
                                .boxed_local(),
                            );
                        }
                    }
                }
            }
            .boxed_local()
        });
    }
}

/// JSON RPC Request Id
#[derive(Debug, PartialEq, Clone, Hash, Eq, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum Id {
    Null,
    Number(i64),
    Str(String),
}

#[derive(Deserialize)]
struct RawIncomingMessage<'a> {
    id: Option<Id>,
    method: Option<&'a str>,
    params: Option<&'a RawValue>,
    result: Option<&'a RawValue>,
    error: Option<Error>,
}

#[derive(Deserialize)]
struct RawIncomingOwned {
    id: Option<Id>,
    method: Option<String>,
    params: Option<serde_json::Value>,
    result: Option<serde_json::Value>,
    error: Option<Error>,
}

/// Incoming message decoded for a given side. Exposed so embedders can drive
/// the dispatcher using custom transports (e.g., WebSockets) while reusing the
/// SDK's decoding and handler wiring.
pub enum IncomingMessage<Local: Side> {
    Request { id: Id, request: Local::InRequest },
    Notification { notification: Local::InNotification },
}

/// Wrapper that carries both the typed request/notification and the raw params
/// (JSON object) used to decode it. Useful for embedders that need access to
/// extension fields not represented in the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedRequest<T> {
    pub typed: T,
    pub raw: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum OutgoingMessage<Local: Side, Remote: Side> {
    Request {
        id: Id,
        method: Arc<str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<Remote::InRequest>,
    },
    Response {
        id: Id,
        #[serde(flatten)]
        result: ResponseResult<Local::OutResponse>,
    },
    Notification {
        method: Arc<str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<Remote::InNotification>,
    },
}

/// Either [`OutgoingMessage`] or [`IncomingMessage`] with `"jsonrpc": "2.0"` specified as
/// [required by JSON-RPC 2.0 Specification][1].
///
/// [1]: https://www.jsonrpc.org/specification#compatibility
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcMessage<M> {
    jsonrpc: &'static str,
    #[serde(flatten)]
    message: M,
}

/// Helper that exposes a Value-based driver around the dispatcher so custom
/// transports (WebSockets, HTTP streaming, etc.) can feed parsed JSON values
/// into the SDK without using the built-in line-oriented IO task. The driver
/// keeps the same request/notification decoding semantics and surfaces decoded
/// messages via an `UnboundedReceiver`.
pub struct ValueDispatcher<Local: Side, Remote: Side> {
    dispatcher: RpcDispatcher<Local, Remote>,
    incoming_tx: UnboundedSender<IncomingMessage<Local>>,
    incoming_rx: Option<UnboundedReceiver<IncomingMessage<Local>>>,
    pending_responses: Arc<Mutex<HashMap<Id, PendingResponse>>>,
    broadcast: StreamSender,
}

impl<Local: Side, Remote: Side> ValueDispatcher<Local, Remote> {
    /// Create a new value-driven dispatcher alongside a receiver for decoded
    /// incoming messages and a stream receiver for broadcast/telemetry.
    pub fn new() -> (Self, StreamReceiver) {
        let (incoming_tx, incoming_rx) = mpsc::unbounded();
        let pending_responses = Arc::new(Mutex::new(HashMap::default()));
        let (broadcast, broadcast_state) = StreamBroadcast::new();
        let rx = broadcast_state.receiver();
        (
            Self {
                dispatcher: RpcDispatcher::new(),
                incoming_tx,
                incoming_rx: Some(incoming_rx),
                pending_responses,
                broadcast,
            },
            rx,
        )
    }
}

impl<Local: Side, Remote: Side> ValueDispatcher<Local, Remote> {
    /// Feed a parsed JSON value into the dispatcher. Returns an optional JSON
    /// value that should be sent back to the remote peer immediately (e.g.,
    /// decoding errors or responses to requests initiated by the local side).
    pub async fn handle_json(&mut self, value: Value) -> Option<Value> {
        self.dispatcher
            .handle_value(
                value,
                &self.incoming_tx,
                &self.pending_responses,
                &self.broadcast,
            )
            .await
    }

    /// Return the receiver for decoded incoming messages.
    pub fn take_incoming(&mut self) -> UnboundedReceiver<IncomingMessage<Local>> {
        self.incoming_rx.take().unwrap_or_else(|| mpsc::unbounded().1)
    }

    /// Format an outgoing message (response/notification) as a JSON value.
    pub fn outgoing_to_value(
        &self,
        message: &OutgoingMessage<Local, Remote>,
    ) -> Result<Value, Error> {
        self.dispatcher.outgoing_to_value(message)
    }

    /// Access the broadcast stream sender (useful for tests/telemetry).
    pub(crate) fn broadcast(&self) -> StreamSender {
        self.broadcast.clone()
    }
}

impl<M> JsonRpcMessage<M> {
    /// Used version of [JSON-RPC protocol].
    ///
    /// [JSON-RPC]: https://www.jsonrpc.org
    pub const VERSION: &'static str = "2.0";

    /// Wraps the provided [`OutgoingMessage`] or [`IncomingMessage`] into a versioned
    /// [`JsonRpcMessage`].
    #[must_use]
    pub fn wrap(message: M) -> Self {
        Self {
            jsonrpc: Self::VERSION,
            message,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ResponseResult<Res> {
    Result(Res),
    Error(Error),
}

impl<T> From<Result<T, Error>> for ResponseResult<T> {
    fn from(result: Result<T, Error>) -> Self {
        match result {
            Ok(value) => ResponseResult::Result(value),
            Err(error) => ResponseResult::Error(error),
        }
    }
}

pub trait Side: Clone {
    type InRequest: Clone + Serialize + DeserializeOwned + 'static;
    type OutResponse: Clone + Serialize + DeserializeOwned + 'static;
    type InNotification: Clone + Serialize + DeserializeOwned + 'static;

    fn decode_request(method: &str, params: Option<&RawValue>) -> Result<Self::InRequest, Error>;

    fn decode_notification(
        method: &str,
        params: Option<&RawValue>,
    ) -> Result<Self::InNotification, Error>;
}

pub trait MessageHandler<Local: Side> {
    fn handle_request(
        &self,
        request: Local::InRequest,
    ) -> impl Future<Output = Result<Local::OutResponse, Error>>;

    fn handle_notification(
        &self,
        notification: Local::InNotification,
    ) -> impl Future<Output = Result<(), Error>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::{Number, Value};

    #[test]
    fn id_deserialization() {
        let id = serde_json::from_value::<Id>(Value::Null).unwrap();
        assert_eq!(id, Id::Null);

        let id =
            serde_json::from_value::<Id>(Value::Number(Number::from_u128(1).unwrap())).unwrap();
        assert_eq!(id, Id::Number(1));

        let id =
            serde_json::from_value::<Id>(Value::Number(Number::from_i128(-1).unwrap())).unwrap();
        assert_eq!(id, Id::Number(-1));

        let id = serde_json::from_value::<Id>(Value::String("id".to_owned())).unwrap();
        assert_eq!(id, Id::Str("id".to_owned()));
    }

    #[test]
    fn id_serialization() {
        let id = serde_json::to_value(Id::Null).unwrap();
        assert_eq!(id, Value::Null);

        let id = serde_json::to_value(Id::Number(1)).unwrap();
        assert_eq!(id, Value::Number(Number::from_u128(1).unwrap()));

        let id = serde_json::to_value(Id::Number(-1)).unwrap();
        assert_eq!(id, Value::Number(Number::from_i128(-1).unwrap()));

        let id = serde_json::to_value(Id::Str("id".to_owned())).unwrap();
        assert_eq!(id, Value::String("id".to_owned()));
    }
}
