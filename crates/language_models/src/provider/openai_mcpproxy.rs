//! OpenAI-compatible provider hardened for MCP-proxy (mcpproxy) tool routing.
//!
//! This is a sibling of `open_ai_compatible` that targets endpoints fronted by
//! an mcpproxy compact router (e.g. `mcpproxy-sovereign` at `:25109`). It diverges
//! from the generic provider in exactly the ways those endpoints require:
//!
//! * `tool_input_format` is **full JSON Schema** (draft07), NOT the OpenAI subset.
//!   The subset transform collapses `type: ["string","null"]` to its first entry
//!   and rewrites `oneOf` -> `anyOf`. vLLM/Outlines (the tool-call grammar parser
//!   behind NVIDIA NIM and many OpenAI-compatible backends) 500s on the result
//!   ("Could not translate instance to regex"), which makes Zed retry forever.
//! * `interleaved_reasoning` is forced on: mcpproxy endpoints that proxy reasoning
//!   models (Inkling) return thinking in a dedicated `reasoning_content` field.
//! * `prompt_cache_key` is never sent (NVIDIA/mcpproxy return 400 on it).
//! * Tool schemas are normalized (non-destructively) so Outlines can compile them:
//!   root `type: object`, explicitly typed properties, bare `null` stripped from
//!   multi-type arrays. No tool-count cap, no description truncation — the mcpproxy
//!   compact router is what keeps the *set* small via progressive disclosure
//!   (`retrieve_tools` / `describe_tool`); the provider must never drop a tool.

use anyhow::Result;
use credentials_provider::CredentialsProvider;
use futures::{FutureExt, StreamExt, future::BoxFuture};
use gpui::{App, AppContext, AsyncApp, Entity, Task};
use http_client::{CustomHeaders, HttpClient};
use language_model::{
    AuthenticateError, IconOrSvg, LanguageModel, LanguageModelCompletionError,
    LanguageModelCompletionEvent, LanguageModelEffortLevel, LanguageModelId, LanguageModelName,
    LanguageModelProvider, LanguageModelProviderId, LanguageModelProviderName,
    LanguageModelProviderState, LanguageModelRequest, LanguageModelToolChoice, LanguageModelToolUse, LanguageModelToolUseInput,
    LanguageModelToolSchemaFormat, ProviderSettingsView, RateLimiter, SubPageProviderSettings,
};
use open_ai::{
    ResponseStreamEvent,
    responses::{Request as ResponseRequest, StreamEvent as ResponsesStreamEvent, stream_response},
    stream_completion,
};
use settings::Settings;
use std::sync::Arc;
use ui::IconName;

use crate::provider::api_compatible::{
    ApiCompatibleProviderConfigurationView, ApiCompatibleProviderSettings,
    ApiCompatibleProviderState,
};
use crate::provider::open_ai::{
    ChatCompletionMaxTokensParameter, OpenAiEventMapper, OpenAiResponseEventMapper, into_open_ai, into_open_ai_response,
};
pub use settings::OpenAiCompatibleAvailableModel as AvailableModel;
pub use settings::OpenAiCompatibleModelCapabilities as ModelCapabilities;

const API_KEY_PLACEHOLDER: &str = "000000000000000000000000000000000000000000000000000";

#[derive(Default, Clone, Debug, PartialEq)]
pub struct OpenAiMcpProxySettings {
    pub api_url: String,
    pub available_models: Vec<AvailableModel>,
    pub custom_headers: CustomHeaders,
}


impl ApiCompatibleProviderSettings for OpenAiMcpProxySettings {
    fn api_url(&self) -> &str {
        &self.api_url
    }
}

pub type State = ApiCompatibleProviderState<OpenAiMcpProxySettings>;

pub struct OpenAiMcpProxyLanguageModelProvider {
    id: LanguageModelProviderId,
    name: LanguageModelProviderName,
    http_client: Arc<dyn HttpClient>,
    state: Entity<State>,
}

impl OpenAiMcpProxyLanguageModelProvider {
    pub fn new(
        id: Arc<str>,
        http_client: Arc<dyn HttpClient>,
        credentials_provider: Arc<dyn CredentialsProvider>,
        cx: &mut App,
    ) -> Self {
        let state = State::new(
            id.clone(),
            credentials_provider,
            |id, cx| {
                crate::AllLanguageModelSettings::get_global(cx)
                    .openai_mcpproxy
                    .get(id)
            },
            cx,
        );

        Self {
            id: id.clone().into(),
            name: id.into(),
            http_client,
            state,
        }
    }

    fn create_language_model(&self, model: AvailableModel) -> Arc<dyn LanguageModel> {
        Arc::new(OpenAiMcpProxyLanguageModel {
            id: LanguageModelId::from(model.name.clone()),
            provider_id: self.id.clone(),
            provider_name: self.name.clone(),
            model,
            state: self.state.clone(),
            http_client: self.http_client.clone(),
            request_limiter: RateLimiter::new(4),
        })
    }
}

impl LanguageModelProviderState for OpenAiMcpProxyLanguageModelProvider {
    type ObservableEntity = State;

    fn observable_entity(&self) -> Option<Entity<Self::ObservableEntity>> {
        Some(self.state.clone())
    }
}

impl LanguageModelProvider for OpenAiMcpProxyLanguageModelProvider {
    fn id(&self) -> LanguageModelProviderId {
        self.id.clone()
    }

    fn name(&self) -> LanguageModelProviderName {
        self.name.clone()
    }

    fn icon(&self) -> IconOrSvg {
        IconOrSvg::Icon(IconName::AiOpenAiCompat)
    }

    fn default_model(&self, cx: &App) -> Option<Arc<dyn LanguageModel>> {
        self.state
            .read(cx)
            .settings
            .available_models
            .first()
            .map(|model| self.create_language_model(model.clone()))
    }

    fn default_fast_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        None
    }

    fn provided_models(&self, cx: &App) -> Vec<Arc<dyn LanguageModel>> {
        self.state
            .read(cx)
            .settings
            .available_models
            .iter()
            .map(|model| self.create_language_model(model.clone()))
            .collect()
    }

    fn is_authenticated(&self, cx: &App) -> bool {
        self.state.read(cx).is_authenticated()
    }

    fn authenticate(&self, cx: &mut App) -> Task<Result<(), AuthenticateError>> {
        self.state.update(cx, |state, cx| state.authenticate(cx))
    }

    fn settings_view(&self, _cx: &mut App) -> Option<ProviderSettingsView> {
        let state = self.state.clone();
        Some(ProviderSettingsView::SubPage(SubPageProviderSettings::new(
            move |window, cx| {
                cx.new(|cx| {
                    ApiCompatibleProviderConfigurationView::new(
                        state.clone(),
                        "OpenAI MCP-Proxy",
                        API_KEY_PLACEHOLDER,
                        window,
                        cx,
                    )
                })
                .into()
            },
        )))
    }

    fn set_api_key(&self, api_key: Option<String>, cx: &mut App) -> Task<Result<()>> {
        self.state
            .update(cx, |state, cx| state.set_api_key(api_key, cx))
    }
}

pub struct OpenAiMcpProxyLanguageModel {
    id: LanguageModelId,
    provider_id: LanguageModelProviderId,
    provider_name: LanguageModelProviderName,
    model: AvailableModel,
    state: Entity<State>,
    http_client: Arc<dyn HttpClient>,
    request_limiter: RateLimiter,
}

impl OpenAiMcpProxyLanguageModel {
    fn stream_completion(
        &self,
        request: open_ai::Request,
        cx: &AsyncApp,
    ) -> BoxFuture<
        'static,
        Result<
            futures::stream::BoxStream<'static, Result<ResponseStreamEvent>>,
            LanguageModelCompletionError,
        >,
    > {
        let http_client = self.http_client.clone();

        let (api_key, api_url, extra_headers) = self.state.read_with(cx, |state, _cx| {
            let api_url = &state.settings.api_url;
            (
                state.api_key_state.key(api_url),
                state.settings.api_url.clone(),
                state.settings.custom_headers.clone(),
            )
        });

        let provider = self.provider_name.clone();
        let future = self.request_limiter.stream(async move {
            let Some(api_key) = api_key else {
                return Err(LanguageModelCompletionError::NoApiKey { provider });
            };
            let request = stream_completion(
                http_client.as_ref(),
                provider.0.as_str(),
                &api_url,
                &api_key,
                request,
                &extra_headers,
            );
            let response = request.await?;
            Ok(response)
        });

        async move { Ok(future.await?.boxed()) }.boxed()
    }

    fn stream_response(
        &self,
        request: ResponseRequest,
        cx: &AsyncApp,
    ) -> BoxFuture<'static, Result<futures::stream::BoxStream<'static, Result<ResponsesStreamEvent>>>
    > {
        let http_client = self.http_client.clone();

        let (api_key, api_url, extra_headers) = self.state.read_with(cx, |state, _cx| {
            let api_url = &state.settings.api_url;
            (
                state.api_key_state.key(api_url),
                state.settings.api_url.clone(),
                state.settings.custom_headers.clone(),
            )
        });

        let provider = self.provider_name.clone();
        let future = self.request_limiter.stream(async move {
            let Some(api_key) = api_key else {
                return Err(LanguageModelCompletionError::NoApiKey { provider });
            };
            let request = stream_response(
                http_client.as_ref(),
                provider.0.as_str(),
                &api_url,
                &api_key,
                request,
                &extra_headers,
            );
            let response = request.await?;
            Ok(response)
        });

        async move { Ok(future.await?.boxed()) }.boxed()
    }
}

fn default_thinking_reasoning_effort(model: &AvailableModel) -> Option<open_ai::ReasoningEffort> {
    model
        .reasoning_effort
        .filter(|effort| *effort != open_ai::ReasoningEffort::None)
}

fn supported_thinking_effort_levels(model: &AvailableModel) -> Vec<LanguageModelEffortLevel> {
    let Some(default_effort) = default_thinking_reasoning_effort(model) else {
        return Vec::new();
    };
    let mut levels = vec![
        LanguageModelEffortLevel {
            name: "low".into(),
            value: "low".into(),
            is_default: false,
        },
        LanguageModelEffortLevel {
            name: "medium".into(),
            value: "medium".into(),
            is_default: false,
        },
        LanguageModelEffortLevel {
            name: "high".into(),
            value: "high".into(),
            is_default: false,
        },
        LanguageModelEffortLevel {
            name: "xhigh".into(),
            value: "xhigh".into(),
            is_default: false,
        },
        LanguageModelEffortLevel {
            name: "max".into(),
            value: "max".into(),
            is_default: false,
        },
    ];
    for level in levels.iter_mut() {
        if level.value == default_effort.value().as_ref() {
            level.is_default = true;
        }
    }
    levels
}

fn chat_completion_max_tokens_parameter(model: &AvailableModel) -> ChatCompletionMaxTokensParameter {
    if model.capabilities.max_tokens_parameter {
        ChatCompletionMaxTokensParameter::MaxTokens
    } else {
        ChatCompletionMaxTokensParameter::MaxCompletionTokens
    }
}

fn supports_none_reasoning_effort(model: &AvailableModel) -> bool {
    model.reasoning_effort.is_some()
}

fn chat_completion_reasoning_effort(
    request: &LanguageModelRequest,
    model: &AvailableModel,
) -> Option<open_ai::ReasoningEffort> {
    if model.reasoning_effort == Some(open_ai::ReasoningEffort::None) {
        return Some(open_ai::ReasoningEffort::None);
    }

    if let Some(request_effort) = request.thinking_effort.as_deref() {
        if let Ok(parsed) = request_effort.parse::<open_ai::ReasoningEffort>() {
            return Some(parsed);
        }
    }

    if supports_none_reasoning_effort(model) {
        return default_thinking_reasoning_effort(model);
    }

    None
}

fn disable_response_thinking_for_none_effort(
    request: &mut LanguageModelRequest,
    model: &AvailableModel,
) {
    if model.reasoning_effort == Some(open_ai::ReasoningEffort::None) {
        request.thinking_effort = None;
    }
}

impl LanguageModel for OpenAiMcpProxyLanguageModel {
    fn id(&self) -> LanguageModelId {
        self.id.clone()
    }

    fn name(&self) -> LanguageModelName {
        LanguageModelName::from(
            self.model
                .display_name
                .clone()
                .unwrap_or_else(|| self.model.name.clone()),
        )
    }

    fn provider_id(&self) -> LanguageModelProviderId {
        self.provider_id.clone()
    }

    fn provider_name(&self) -> LanguageModelProviderName {
        self.provider_name.clone()
    }

    fn supports_tools(&self) -> bool {
        self.model.capabilities.tools
    }

    fn supports_autonomous_edits(&self) -> bool {
        self.model.capabilities.autonomous_edits
    }

    fn tool_input_format(&self) -> LanguageModelToolSchemaFormat {
        // Full JSON Schema (draft07). The subset transform collapses nullable /
        // oneOf schemas and makes Outlines 500 -> Zed retries forever.
        LanguageModelToolSchemaFormat::JsonSchema
    }

    fn supports_images(&self) -> bool {
        self.model.capabilities.images
    }

    fn supports_tool_choice(&self, choice: LanguageModelToolChoice) -> bool {
        match choice {
            LanguageModelToolChoice::Auto => self.model.capabilities.tools,
            LanguageModelToolChoice::Any => self.model.capabilities.tools,
            LanguageModelToolChoice::None => true,
        }
    }

    fn supports_streaming_tools(&self) -> bool {
        true
    }

    fn supports_thinking(&self) -> bool {
        default_thinking_reasoning_effort(&self.model).is_some()
    }

    fn supported_effort_levels(&self) -> Vec<LanguageModelEffortLevel> {
        supported_thinking_effort_levels(&self.model)
    }

    fn supports_split_token_display(&self) -> bool {
        true
    }

    fn telemetry_id(&self) -> String {
        format!("openai-mcpproxy/{}", self.model.name)
    }

    fn max_token_count(&self) -> u64 {
        self.model.max_tokens
    }

    fn max_output_tokens(&self) -> Option<u64> {
        self.model.max_output_tokens
    }

    fn stream_completion(
        &self,
        mut request: LanguageModelRequest,
        cx: &AsyncApp,
    ) -> BoxFuture<
        'static,
        Result<
            futures::stream::BoxStream<
                'static,
                Result<LanguageModelCompletionEvent, LanguageModelCompletionError>,
            >,
            LanguageModelCompletionError,
        >,
    > {
        if !self.supports_fast_mode() {
            request.speed = None;
        }

        // Non-destructive schema normalization so Outlines can compile every
        // tool schema (no count cap, no description truncation).
        request.tools = normalize_tool_schemas(request.tools);

        if self.model.capabilities.chat_completions {
            let reasoning_effort = chat_completion_reasoning_effort(&request, &self.model);
            let request = match into_open_ai(
                request,
                &self.model.name,
                self.model.capabilities.parallel_tool_calls,
                false, // prompt_cache_key: mcpproxy/NVIDIA return 400 on this
                self.max_output_tokens(),
                chat_completion_max_tokens_parameter(&self.model),
                reasoning_effort,
                // interleaved_reasoning: Inkling accepts a message-level `reasoning_content`
                // field (proven via curl). Enabling this round-trips prior thinking on
                // multi-turn turns. This is NOT a top-level request param -- `into_open_ai`
                // only populates the Assistant message's `reasoning_content` field.
                true,
            ) {
                Ok(request) => request,
                Err(error) => return async move { Err(error.into()) }.boxed(),
            };
            let completions = self.stream_completion(request, cx);
            async move {
                let mapper = McpProxyEventMapper::new();
                Ok(mapper.map_stream(completions.await?))
            }
            .boxed()
        } else {
            disable_response_thinking_for_none_effort(&mut request, &self.model);
            let request = into_open_ai_response(
                request,
                &self.model.name,
                self.model.capabilities.parallel_tool_calls,
                false, // prompt_cache_key
                self.max_output_tokens(),
                default_thinking_reasoning_effort(&self.model),
                supports_none_reasoning_effort(&self.model),
            );
            let completions = self.stream_response(request, cx);
            async move {
                let mapper = OpenAiResponseEventMapper::new();
                Ok(mapper.map_stream(completions.await?).boxed())
            }
            .boxed()
        }
    }
}

/// Non-destructive JSON-Schema normalizer for Outlines-backed endpoints.
///
/// Repairs structural defects that make Outlines 500 ("Could not translate
/// instance to regex"): missing root `type`, untyped properties, and bare
/// `"null"` entries in multi-type arrays. Returns every tool unchanged
/// otherwise — no count cap, no description truncation.
fn normalize_tool_schemas(
    tools: Vec<language_model::LanguageModelRequestTool>,
) -> Vec<language_model::LanguageModelRequestTool> {
    tools
        .into_iter()
        .map(|mut tool| {
            if let language_model::LanguageModelRequestToolInput::Function { input_schema, .. } =
                &mut tool.input
            {
                *input_schema = normalize_schema(input_schema.clone());
            }
            tool
        })
        .collect()
}

fn normalize_schema(mut schema: serde_json::Value) -> serde_json::Value {
    if !schema.is_object() {
        return serde_json::json!({ "type": "object" });
    }

    let root_type = schema.get("type").cloned();
    let is_explicit_object = matches!(root_type, Some(serde_json::Value::String(s)) if s == "object");
    if !is_explicit_object {
        schema["type"] = serde_json::json!("object");
    }

    if let Some(props) = schema.get_mut("properties").and_then(|p| p.as_object_mut()) {
        for (_name, prop) in props.iter_mut() {
            if prop.is_object() {
                let t = prop.get("type").cloned();
                let has_type = match &t {
                    Some(serde_json::Value::String(s)) => !s.is_empty() && s != "null",
                    Some(serde_json::Value::Array(a)) => {
                        let cleaned: Vec<_> = a
                            .iter()
                            .filter(|v| !matches!(v, serde_json::Value::String(s) if s == "null"))
                            .cloned()
                            .collect();
                        let non_empty = !cleaned.is_empty();
                        prop["type"] = serde_json::Value::Array(cleaned);
                        non_empty
                    }
                    _ => false,
                };
                if !has_type {
                    prop["type"] = serde_json::json!("string");
                }
                if matches!(prop.get("type"), Some(serde_json::Value::String(s)) if s == "object") {
                    *prop = normalize_schema(prop.clone());
                }
            }
        }
    }
    schema
}

/// Self-healing event mapper for mcpproxy-fronted endpoints.
///
/// Wraps `OpenAiEventMapper` and converts a `ToolUseJsonParseError` (which
/// happens when a model streams a `tool_calls` delta whose `arguments` never
/// close) into a valid `ToolUse` with empty `{}` input, preventing the infinite
/// retry loop when a model forgets to emit tool arguments.
struct McpProxyEventMapper {
    inner: OpenAiEventMapper,
}

impl McpProxyEventMapper {
    fn new() -> Self {
        Self { inner: OpenAiEventMapper::new() }
    }

    fn map_stream(
        self,
        events: futures::stream::BoxStream<'static, Result<ResponseStreamEvent>>,
    ) -> futures::stream::BoxStream<'static, Result<LanguageModelCompletionEvent, LanguageModelCompletionError>>
    {
        self.inner
            .map_stream(events)
            .map(|event| match event {
                Ok(LanguageModelCompletionEvent::ToolUseJsonParseError {
                    id,
                    tool_name,
                    raw_input,
                    ..
                }) => {
                    log::warn!(
                        "openai-mcpproxy: auto-recovering malformed tool call `{tool_name}` \
                         (arguments did not parse) as empty input instead of retrying"
                    );
                    Ok(LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse {
                        id,
                        name: tool_name,
                        is_input_complete: true,
                        input: LanguageModelToolUseInput::Json(serde_json::Value::Object(
                            serde_json::Map::new(),
                        )),
                        raw_input: raw_input.to_string(),
                        thought_signature: None,
                    }))
                }
                other => other,
            })
            .boxed()
    }
}
