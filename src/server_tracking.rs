use crate::auth::{self, AuthCredential};
use crate::{analytics_attribution, errors, tracking, CodeSceneServer};
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RecordedTrackingCall {
    Event {
        name: String,
        context: analytics_attribution::AnalyticsContext,
    },
    Error {
        tool: String,
        error_kind: String,
        context: analytics_attribution::AnalyticsContext,
    },
}

#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct TrackingProbe(Arc<std::sync::Mutex<Vec<RecordedTrackingCall>>>);

#[cfg(test)]
impl TrackingProbe {
    pub(crate) fn calls(&self) -> Vec<RecordedTrackingCall> {
        self.0.lock().unwrap().clone()
    }

    fn record(&self, call: RecordedTrackingCall) {
        self.0.lock().unwrap().push(call);
    }

    pub(crate) fn install(server: &mut CodeSceneServer) -> Self {
        let probe = Self::default();
        server.tracking_probe = Some(probe.clone());
        probe
    }

    pub(crate) fn assert_single(&self, expected: RecordedTrackingCall) {
        assert_eq!(self.calls(), [expected]);
    }
}

pub(crate) struct ContextualErrorEvent<'a> {
    pub(crate) error_kind: &'a str,
    pub(crate) tool: &'a str,
    pub(crate) detail: Option<&'a str>,
    pub(crate) context: analytics_attribution::AnalyticsContext,
}

impl<'a> ContextualErrorEvent<'a> {
    pub(crate) fn for_project(error_kind: &'a str, tool: &'a str, project_id: i64) -> Self {
        Self {
            error_kind,
            tool,
            detail: None,
            context: analytics_attribution::AnalyticsContext::ExplicitProjectIds(vec![project_id]),
        }
    }
}

impl CodeSceneServer {
    pub(crate) fn track(&self, event: &str, props: serde_json::Value) {
        self.track_with_context(
            event,
            props,
            analytics_attribution::AnalyticsContext::CurrentWorkspace,
        );
    }

    pub(crate) fn track_with_context(
        &self,
        event: &str,
        props: serde_json::Value,
        context: analytics_attribution::AnalyticsContext,
    ) {
        if tracking::is_disabled() {
            return;
        }
        #[cfg(test)]
        if let Some(probe) = &self.tracking_probe {
            probe.record(RecordedTrackingCall::Event {
                name: event.to_string(),
                context,
            });
            return;
        }
        let (auth, credential) = self.tracking_auth();
        tracking::track_event_with_attribution(
            tracking::AttributedEvent {
                event,
                properties: props,
                instance_id: &self.instance_id,
                auth: &auth,
                attribution: tracking::TrackingAttribution {
                    context,
                    credential,
                    http_client: self.http_client.clone(),
                    cache: self.repository_projects_cache.clone(),
                },
            },
            &self.tracking_tasks,
        );
    }

    pub(crate) fn track_api_err(&self, tool: &str, err: &errors::ApiError) {
        tracing::warn!(tool, error = %err, "API error");
        self.track_error_with_context(ContextualErrorEvent {
            error_kind: err.kind(),
            tool,
            detail: None,
            context: analytics_attribution::AnalyticsContext::CurrentWorkspace,
        });
    }

    pub(crate) fn track_err_msg(&self, tool: &str, error_kind: &str, err: &str) {
        self.track_contextual_err(
            ContextualErrorEvent {
                error_kind,
                tool,
                detail: None,
                context: analytics_attribution::AnalyticsContext::CurrentWorkspace,
            },
            &err,
        );
    }

    pub(crate) fn track_contextual_err(
        &self,
        event: ContextualErrorEvent<'_>,
        error: &dyn std::fmt::Display,
    ) {
        tracing::warn!(tool = event.tool, error = %error, "tool error");
        self.track_error_with_context(event);
    }

    pub(crate) fn track_error_with_context(&self, event: ContextualErrorEvent<'_>) {
        if tracking::is_disabled() {
            return;
        }
        #[cfg(test)]
        if let Some(probe) = &self.tracking_probe {
            probe.record(RecordedTrackingCall::Error {
                tool: event.tool.to_string(),
                error_kind: event.error_kind.to_string(),
                context: event.context,
            });
            return;
        }
        let (auth, credential) = self.tracking_auth();
        tracking::track_error_with_attribution(
            &tracking::ErrorEvent {
                error_kind: event.error_kind,
                tool_name: event.tool,
                instance_id: &self.instance_id,
                detail: event.detail,
                auth: &auth,
            },
            tracking::TrackingAttribution {
                context: event.context,
                credential,
                http_client: self.http_client.clone(),
                cache: self.repository_projects_cache.clone(),
            },
            &self.tracking_tasks,
        );
    }

    /// Resolve the best available token and API root for tracking.
    /// Prefers configured PAT, falls back to cached OAuth token.
    fn tracking_auth(&self) -> (tracking::TrackingAuth, Option<AuthCredential>) {
        let credential =
            auth::configured_credential().or_else(|| self.auth_manager.try_cached_credential());
        let auth = tracking::TrackingAuth {
            access_token: credential
                .as_ref()
                .map(|credential| credential.access_token().to_string())
                .unwrap_or_default(),
            api_root: credential
                .as_ref()
                .and_then(|credential| credential.api_root().ok()),
        };
        (auth, credential)
    }
}
