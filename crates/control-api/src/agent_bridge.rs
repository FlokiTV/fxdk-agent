use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    sync::{Mutex, oneshot},
    time::timeout,
};
use utoipa::ToSchema;

pub const DEFAULT_AGENT_TIMEOUT_MS: u64 = 5_000;
pub const MAX_AGENT_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentInvokeRequest {
    #[serde(default = "default_client_id")]
    pub client_id: u32,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    pub timeout_ms: Option<u64>,
}

fn default_client_id() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentInvokeResponse {
    pub request_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AgentRuntimeError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeRegistration {
    pub client_id: u32,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilitiesResponse {
    pub ready: bool,
    pub client_id: Option<u32>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeRequest {
    pub request_id: String,
    pub client_id: u32,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeResponse {
    pub request_id: String,
    pub client_id: u32,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AgentRuntimeError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentBridgeError {
    RuntimeUnavailable,
    WrongClient,
    MethodUnsupported,
    Timeout,
    ResponseChannelClosed,
    UnknownRequest,
}

#[derive(Clone, Default)]
pub struct AgentBridge {
    inner: Arc<Mutex<AgentBridgeState>>,
    request_counter: Arc<AtomicU64>,
}

#[derive(Default)]
struct AgentBridgeState {
    runtime: Option<AgentRuntimeRegistration>,
    queue: VecDeque<AgentRuntimeRequest>,
    pending: HashMap<String, oneshot::Sender<AgentRuntimeResponse>>,
}

impl AgentBridge {
    pub async fn register(
        &self,
        registration: AgentRuntimeRegistration,
    ) -> AgentCapabilitiesResponse {
        let mut state = self.inner.lock().await;

        if state
            .runtime
            .as_ref()
            .is_some_and(|current| current.client_id != registration.client_id)
        {
            state.queue.clear();
            state.pending.clear();
        }

        state.runtime = Some(AgentRuntimeRegistration {
            client_id: registration.client_id,
            capabilities: normalized_capabilities(registration.capabilities),
        });

        capabilities_snapshot(&state)
    }

    pub async fn capabilities(&self) -> AgentCapabilitiesResponse {
        let state = self.inner.lock().await;
        capabilities_snapshot(&state)
    }

    pub async fn reset(&self) {
        let mut state = self.inner.lock().await;
        state.runtime = None;
        state.queue.clear();
        state.pending.clear();
    }

    pub async fn next_request(
        &self,
        client_id: u32,
    ) -> Result<Option<AgentRuntimeRequest>, AgentBridgeError> {
        let mut state = self.inner.lock().await;
        let runtime = state
            .runtime
            .as_ref()
            .ok_or(AgentBridgeError::RuntimeUnavailable)?;

        if runtime.client_id != client_id {
            return Err(AgentBridgeError::WrongClient);
        }

        Ok(state.queue.pop_front())
    }

    pub async fn respond(&self, response: AgentRuntimeResponse) -> Result<(), AgentBridgeError> {
        let sender = {
            let mut state = self.inner.lock().await;
            let runtime = state
                .runtime
                .as_ref()
                .ok_or(AgentBridgeError::RuntimeUnavailable)?;

            if runtime.client_id != response.client_id {
                return Err(AgentBridgeError::WrongClient);
            }

            state
                .pending
                .remove(&response.request_id)
                .ok_or(AgentBridgeError::UnknownRequest)?
        };

        sender
            .send(response)
            .map_err(|_| AgentBridgeError::ResponseChannelClosed)
    }

    pub async fn invoke(
        &self,
        request: AgentInvokeRequest,
    ) -> Result<AgentInvokeResponse, AgentBridgeError> {
        let timeout_ms = request
            .timeout_ms
            .unwrap_or(DEFAULT_AGENT_TIMEOUT_MS)
            .clamp(1, MAX_AGENT_TIMEOUT_MS);
        let request_id = format!(
            "agent-{:08}",
            self.request_counter.fetch_add(1, Ordering::Relaxed) + 1
        );
        let runtime_request = AgentRuntimeRequest {
            request_id: request_id.clone(),
            client_id: request.client_id,
            method: request.method,
            params: request.params,
        };
        let (sender, receiver) = oneshot::channel();

        {
            let mut state = self.inner.lock().await;
            let runtime = state
                .runtime
                .as_ref()
                .ok_or(AgentBridgeError::RuntimeUnavailable)?;

            if runtime.client_id != runtime_request.client_id {
                return Err(AgentBridgeError::WrongClient);
            }

            if !runtime
                .capabilities
                .iter()
                .any(|capability| capability == &runtime_request.method)
            {
                return Err(AgentBridgeError::MethodUnsupported);
            }

            state.pending.insert(request_id.clone(), sender);
            state.queue.push_back(runtime_request);
        }

        match timeout(Duration::from_millis(timeout_ms), receiver).await {
            Ok(Ok(response)) => Ok(AgentInvokeResponse {
                request_id: response.request_id,
                ok: response.ok,
                result: response.result,
                error: response.error,
            }),
            Ok(Err(_)) => Err(AgentBridgeError::ResponseChannelClosed),
            Err(_) => {
                let mut state = self.inner.lock().await;
                state.pending.remove(&request_id);
                state.queue.retain(|queued| queued.request_id != request_id);
                Err(AgentBridgeError::Timeout)
            }
        }
    }
}

fn normalized_capabilities(mut capabilities: Vec<String>) -> Vec<String> {
    capabilities.retain(|capability| !capability.trim().is_empty());
    capabilities.sort();
    capabilities.dedup();
    capabilities
}

fn capabilities_snapshot(state: &AgentBridgeState) -> AgentCapabilitiesResponse {
    match &state.runtime {
        Some(runtime) => AgentCapabilitiesResponse {
            ready: true,
            client_id: Some(runtime.client_id),
            capabilities: runtime.capabilities.clone(),
        },
        None => AgentCapabilitiesResponse {
            ready: false,
            client_id: None,
            capabilities: Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        AgentBridge, AgentBridgeError, AgentInvokeRequest, AgentRuntimeRegistration,
        AgentRuntimeResponse,
    };

    #[tokio::test]
    async fn register_normalizes_capabilities() {
        let bridge = AgentBridge::default();
        let snapshot = bridge
            .register(AgentRuntimeRegistration {
                client_id: 1,
                capabilities: vec![
                    "runtime.status".to_owned(),
                    "runtime.ping".to_owned(),
                    "runtime.ping".to_owned(),
                ],
            })
            .await;

        assert!(snapshot.ready);
        assert_eq!(snapshot.client_id, Some(1));
        assert_eq!(
            snapshot.capabilities,
            vec!["runtime.ping".to_owned(), "runtime.status".to_owned()]
        );
    }

    #[tokio::test]
    async fn request_round_trip_correlates_by_request_id() {
        let bridge = AgentBridge::default();
        bridge
            .register(AgentRuntimeRegistration {
                client_id: 1,
                capabilities: vec!["runtime.ping".to_owned()],
            })
            .await;

        let invoke_bridge = bridge.clone();
        let invoke = tokio::spawn(async move {
            invoke_bridge
                .invoke(AgentInvokeRequest {
                    client_id: 1,
                    method: "runtime.ping".to_owned(),
                    params: json!({"value": 7}),
                    timeout_ms: Some(1_000),
                })
                .await
        });

        let mut request = None;
        for _ in 0..20 {
            request = bridge.next_request(1).await.expect("next request");
            if request.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let request = request.expect("queued request");
        assert_eq!(request.method, "runtime.ping");
        assert_eq!(request.params, json!({"value": 7}));

        bridge
            .respond(AgentRuntimeResponse {
                request_id: request.request_id.clone(),
                client_id: 1,
                ok: true,
                result: Some(json!({"pong": true})),
                error: None,
            })
            .await
            .expect("respond");

        let response = invoke.await.expect("join").expect("invoke");
        assert_eq!(response.request_id, request.request_id);
        assert!(response.ok);
        assert_eq!(response.result, Some(json!({"pong": true})));
    }

    #[tokio::test]
    async fn reset_cancels_pending_request_and_registration() {
        let bridge = AgentBridge::default();
        bridge
            .register(AgentRuntimeRegistration {
                client_id: 1,
                capabilities: vec!["runtime.status".to_owned()],
            })
            .await;

        let invoke_bridge = bridge.clone();
        let invoke = tokio::spawn(async move {
            invoke_bridge
                .invoke(AgentInvokeRequest {
                    client_id: 1,
                    method: "runtime.status".to_owned(),
                    params: json!({}),
                    timeout_ms: Some(1_000),
                })
                .await
        });

        for _ in 0..20 {
            if bridge
                .next_request(1)
                .await
                .expect("next request")
                .is_some()
            {
                break;
            }
            tokio::task::yield_now().await;
        }

        bridge.reset().await;

        assert_eq!(
            invoke.await.expect("join").expect_err("reset must cancel"),
            AgentBridgeError::ResponseChannelClosed
        );
        assert!(!bridge.capabilities().await.ready);
    }
}
