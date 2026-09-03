//! NVIDIA NIM (Inkling) language model provider.
//!
//! NVIDIA's `integrate.api.nvidia.com/v1/chat/completions` endpoint is
//! OpenAI-compatible, so this provider reuses the `open_ai` crate's request
//! serialization, streaming, and event mapping. It diverges from the generic
//! `openai_compatible` provider in the ways NVIDIA's vLLM/Outlines backend
//! requires:
//!
//! * `tool_input_format` is **full JSON Schema**, not the OpenAI subset. NVIDIA's
//!   Outlines tool parser rejects the subset form and 500s on large tool sets
//!   (e.g. 120 MCP tools), which previously caused Zed to retry forever.
//! * `interleaved_reasoning` is enabled: Inkling returns thinking in a dedicated
//!   `reasoning_content` field (proven via curl), surfaced as Zed thinking.
//! * `prompt_cache_key` is never sent (NVIDIA returns 400 on it).
//! * `max_tokens` is the native output-token parameter (not `max_completion_tokens`).
//! * Tool definitions are sanitized: descriptions are truncated and the tool
//!   count is capped so Outlines does not explode.

use anyhow::Result;
use collections::BTreeMap;
use credentials_provider::CredentialsProvider;
use futures::{FutureExt, StreamExt, future::BoxFuture};
use gpui::{App, AppContext, AsyncApp, Context, Entity, SharedString, Task};
use http_client::{CustomHeaders, HttpClient};
use language_model::{
    ApiKeyConfiguration, ApiKeyState, AuthenticateError, EnvVar, IconOrSvg, LanguageModel,
    LanguageModelCompletionError, LanguageModelCompletionEvent, LanguageModelEffortLevel,
    LanguageModelId, LanguageModelName, LanguageModelProvider, LanguageModelProviderId,
    LanguageModelProviderName, LanguageModelProviderState, LanguageModelRequest,
    LanguageModelToolChoice, LanguageModelToolSchemaFormat, ProviderSettingsView, RateLimiter,
    env_var,
};
use open_ai::ResponseStreamEvent;
pub use settings::NvidiaAvailableModel as AvailableModel;
use settings::{Settings, SettingsStore};
use std::sync::{Arc, LazyLock};
use strum::IntoEnumIterator;
use ui::IconName;
use nvidia::NVIDIA_API_URL;

const PROVIDER_ID: LanguageModelProviderId = LanguageModelProviderId::new("nvidia");
const PROVIDER_NAME: LanguageModelProviderName = LanguageModelProviderName::new("NVIDIA");

const API_KEY_ENV_VAR_NAME: &str = "NVIDIA_API_KEY";
static API_KEY_ENV_VAR: LazyLock<EnvVar> = env_var!(API_KEY_ENV_VAR_NAME);


#[derive(Default, Clone, Debug, PartialEq)]
pub struct NvidiaSettings {
    pub api_url: String,
    pub available_models: Vec<AvailableModel>,
    pub custom_headers: CustomHeaders,
}

pub struct NvidiaLanguageModelProvider {
    http_client: Arc<dyn HttpClient>,
    state: Entity<State>,
}

pub struct State {
    api_key_state: ApiKeyState,
    credentials_provider: Arc<dyn CredentialsProvider>,
}

impl State {
    fn is_authenticated(&self) -> bool {
        self.api_key_state.has_key()
    }

    fn set_api_key(&mut self, api_key: Option<String>, cx: &mut Context<Self>) -> Task<Result<()>> {
        let credentials_provider = self.credentials_provider.clone();
        let api_url = NvidiaLanguageModelProvider::api_url(cx);
        self.api_key_state.store(
            api_url,
            api_key,
            |this| &mut this.api_key_state,
            credentials_provider,
            cx,
        )
    }

    fn authenticate(&mut self, cx: &mut Context<Self>) -> Task<Result<(), AuthenticateError>> {
        let credentials_provider = self.credentials_provider.clone();
        let api_url = NvidiaLanguageModelProvider::api_url(cx);
        self.api_key_state.load_if_needed(
            api_url,
            |this| &mut this.api_key_state,
            credentials_provider,
            cx,
        )
    }
}

impl NvidiaLanguageModelProvider {
    pub fn new(
        http_client: Arc<dyn HttpClient>,
        credentials_provider: Arc<dyn CredentialsProvider>,
        cx: &mut App,
    ) -> Self {
        let state = cx.new(|cx| {
            cx.observe_global::<SettingsStore>(|this: &mut State, cx| {
                let credentials_provider = this.credentials_provider.clone();
                let api_url = Self::api_url(cx);
                this.api_key_state.handle_url_change(
                    api_url,
                    |this| &mut this.api_key_state,
                    credentials_provider,
                    cx,
                );
                cx.notify();
            })
            .detach();
            State {
                api_key_state: ApiKeyState::new(Self::api_url(cx), (*API_KEY_ENV_VAR).clone()),
                credentials_provider,
            }
        });

        Self { http_client, state }
    }

    fn create_language_model(&self, model: nvidia::Model) -> Arc<dyn LanguageModel> {
        Arc::new(NvidiaLanguageModel {
            id: LanguageModelId::from(model.id().to_string()),
            model,
            state: self.state.clone(),
            http_client: self.http_client.clone(),
            request_limiter: RateLimiter::new(4),
        })
    }

    fn settings(cx: &App) -> &NvidiaSettings {
        &crate::AllLanguageModelSettings::get_global(cx).nvidia
    }

    fn api_url(cx: &App) -> SharedString {
        let api_url = &Self::settings(cx).api_url;
        if api_url.is_empty() {
            NVIDIA_API_URL.into()
        } else {
            SharedString::new(api_url.as_str())
        }
    }
}

impl LanguageModelProviderState for NvidiaLanguageModelProvider {
    type ObservableEntity = State;

    fn observable_entity(&self) -> Option<Entity<Self::ObservableEntity>> {
        Some(self.state.clone())
    }
}

impl LanguageModelProvider for NvidiaLanguageModelProvider {
    fn id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn icon(&self) -> IconOrSvg {
        IconOrSvg::Icon(IconName::AiNvidia)
    }

    fn default_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        Some(self.create_language_model(nvidia::Model::default()))
    }

    fn default_fast_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        Some(self.create_language_model(nvidia::Model::default_fast()))
    }

    fn provided_models(&self, cx: &App) -> Vec<Arc<dyn LanguageModel>> {
        let mut models = BTreeMap::default();

        for model in nvidia::Model::iter() {
            if !matches!(model, nvidia::Model::Custom { .. }) {
                models.insert(model.id().to_string(), model);
            }
        }

        for model in &Self::settings(cx).available_models {
            // Never let a custom entry silently downgrade a built-in model.
            // e.g. a Custom `thinkingmachines/inkling` with missing caps would
            // lose tools/thinking (Model::Custom hardcodes those to false when
            // None). The built-in Inkling is already registered above and is
            // strictly more capable, so we keep it and ignore the duplicate.
            if models.contains_key(&model.name) {
                continue;
            }
            models.insert(
                model.name.clone(),
                nvidia::Model::Custom {
                    name: model.name.clone(),
                    display_name: model.display_name.clone(),
                    max_tokens: model.max_tokens,
                    max_output_tokens: model.max_output_tokens,
                    max_completion_tokens: model.max_completion_tokens,
                    supports_images: model.supports_images,
                    supports_tools: model.supports_tools,
                    parallel_tool_calls: model.parallel_tool_calls,
                },
            );
        }

        models
            .into_values()
            .map(|model| self.create_language_model(model))
            .collect()
    }

    fn is_authenticated(&self, cx: &App) -> bool {
        self.state.read(cx).is_authenticated()
    }

    fn authenticate(&self, cx: &mut App) -> Task<Result<(), AuthenticateError>> {
        self.state.update(cx, |state, cx| state.authenticate(cx))
    }

    fn settings_view(&self, cx: &mut App) -> Option<ProviderSettingsView> {
        let state = self.state.read(cx);
        Some(ProviderSettingsView::ApiKey(ApiKeyConfiguration::new(
            state.api_key_state.has_key(),
            state.api_key_state.is_from_env_var(),
            state.api_key_state.env_var_name().clone(),
            "https://build.nvidia.com/".into(),
        )))
    }

    fn set_api_key(&self, api_key: Option<String>, cx: &mut App) -> Task<Result<()>> {
        self.state
            .update(cx, |state, cx| state.set_api_key(api_key, cx))
    }
}

pub struct NvidiaLanguageModel {
    id: LanguageModelId,
    model: nvidia::Model,
    state: Entity<State>,
    http_client: Arc<dyn HttpClient>,
    request_limiter: RateLimiter,
}

impl NvidiaLanguageModel {
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

        let (api_key, api_url, extra_headers) = self.state.read_with(cx, |state, cx| {
            let api_url = NvidiaLanguageModelProvider::api_url(cx);
            let extra_headers = NvidiaLanguageModelProvider::settings(cx)
                .custom_headers
                .clone();
            (state.api_key_state.key(&api_url), api_url, extra_headers)
        });

        let future = self.request_limiter.stream(async move {
            let provider = PROVIDER_NAME;
            let Some(api_key) = api_key else {
                return Err(LanguageModelCompletionError::NoApiKey { provider });
            };
            let request = open_ai::stream_completion(
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

fn nvidia_reasoning_efforts(model: &nvidia::Model) -> &'static [open_ai::ReasoningEffort] {
    if model.supports_reasoning_effort() {
        &[
            open_ai::ReasoningEffort::None,
            open_ai::ReasoningEffort::Low,
            open_ai::ReasoningEffort::Medium,
            open_ai::ReasoningEffort::High,
            open_ai::ReasoningEffort::XHigh,
            open_ai::ReasoningEffort::Max,
        ]
    } else {
        &[]
    }
}

fn default_thinking_reasoning_effort(model: &nvidia::Model) -> Option<open_ai::ReasoningEffort> {
    if model.supports_reasoning_effort() {
        Some(open_ai::ReasoningEffort::Max)
    } else {
        None
    }
}

fn reasoning_effort_for_request(
    request: &LanguageModelRequest,
    model: &nvidia::Model,
) -> Option<open_ai::ReasoningEffort> {
    let supported_efforts = nvidia_reasoning_efforts(model);
    if supported_efforts.is_empty() {
        return None;
    }

    if request.thinking_allowed {
        request
            .thinking_effort
            .as_deref()
            .and_then(|effort| effort.parse::<open_ai::ReasoningEffort>().ok())
            .filter(|effort| supported_efforts.contains(effort))
            .filter(|effort| *effort != open_ai::ReasoningEffort::None)
            .or_else(|| default_thinking_reasoning_effort(model))
    } else if supported_efforts.contains(&open_ai::ReasoningEffort::None) {
        Some(open_ai::ReasoningEffort::None)
    } else {
        None
    }
}

fn supported_thinking_effort_levels(model: &nvidia::Model) -> Vec<LanguageModelEffortLevel> {
    let default_effort = default_thinking_reasoning_effort(model);
    nvidia_reasoning_efforts(model)
        .iter()
        .copied()
        .filter_map(|effort| {
            let (name, value) = match effort {
                open_ai::ReasoningEffort::None => return None,
                open_ai::ReasoningEffort::Minimal => ("Minimal", "minimal"),
                open_ai::ReasoningEffort::Low => ("Low", "low"),
                open_ai::ReasoningEffort::Medium => ("Medium", "medium"),
                open_ai::ReasoningEffort::High => ("High", "high"),
                open_ai::ReasoningEffort::XHigh => ("Extra High", "xhigh"),
                open_ai::ReasoningEffort::Max => ("Max", "max"),
            };

            Some(LanguageModelEffortLevel {
                name: name.into(),
                value: value.into(),
                is_default: Some(effort) == default_effort,
            })
        })
        .collect()
}

impl LanguageModel for NvidiaLanguageModel {
    fn id(&self) -> LanguageModelId {
        self.id.clone()
    }

    fn name(&self) -> LanguageModelName {
        LanguageModelName::from(self.model.display_name().to_string())
    }

    fn provider_id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn provider_name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn supports_tools(&self) -> bool {
        self.model.supports_tool()
    }

    fn supports_images(&self) -> bool {
        self.model.supports_images()
    }

    fn supports_streaming_tools(&self) -> bool {
        true
    }

    fn supports_tool_choice(&self, choice: LanguageModelToolChoice) -> bool {
        match choice {
            LanguageModelToolChoice::Auto
            | LanguageModelToolChoice::Any
            | LanguageModelToolChoice::None => true,
        }
    }

    fn supports_thinking(&self) -> bool {
        self.model.supports_reasoning_effort()
    }

    fn supported_effort_levels(&self) -> Vec<LanguageModelEffortLevel> {
        supported_thinking_effort_levels(&self.model)
    }

    fn tool_input_format(&self) -> LanguageModelToolSchemaFormat {
        // NVIDIA's Outlines tool parser needs the FULL JSON Schema, not the
        // OpenAI subset that the generic `openai_compatible` provider sends.
        LanguageModelToolSchemaFormat::JsonSchema
    }

    fn telemetry_id(&self) -> String {
        format!("nvidia/{}", self.model.id())
    }

    fn max_token_count(&self) -> u64 {
        self.model.max_token_count()
    }

    fn max_output_tokens(&self) -> Option<u64> {
        self.model.max_output_tokens()
    }

    fn supports_split_token_display(&self) -> bool {
        true
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
        // Normalize tool schemas so NVIDIA's Outlines backend receives valid,
        // full JSON Schema (root object + explicitly typed properties). This is
        // NON-destructive: no tool-count cap, no description truncation. The
        // generic openai_compatible provider instead sends JsonSchemaSubset,
        // which collapses nullable/oneOf and makes Outlines 500 ("Could not
        // translate instance to regex") on large MCP tool sets -> Zed retries
        // forever. We keep the full schema and only repair structural defects.
        request.tools = normalize_tool_schemas(request.tools);

        let reasoning_effort = reasoning_effort_for_request(&request, &self.model);
        let request = match crate::provider::open_ai::into_open_ai(
            request,
            self.model.id(),
            self.model.supports_parallel_tool_calls(),
            false, // prompt_cache_key: NVIDIA returns 400 on this parameter
            self.max_output_tokens(),
            crate::provider::open_ai::ChatCompletionMaxTokensParameter::MaxTokens,
            reasoning_effort,
            // interleaved_reasoning: Inkling accepts a message-level `reasoning_content`
            // field (proven via curl). Enabling this round-trips prior thinking on
            // multi-turn turns instead of dropping it. NOTE: this is NOT a top-level
            // request param -- `into_open_ai` only populates the Assistant message's
            // `reasoning_content` field, which NVIDIA NIM accepts (no 400).
            true,
        ) {
            Ok(request) => request,
            Err(error) => return async move { Err(error.into()) }.boxed(),
        };
        let completions = self.stream_completion(request, cx);
        async move {
            let mapper = crate::provider::open_ai::OpenAiEventMapper::new();
            Ok(mapper.map_stream(completions.await?).boxed())
        }
        .boxed()
    }
}

/// Non-destructive JSON-Schema normalizer for NVIDIA's Outlines tool parser.
///
/// Outlines materializes a regex/grammar from each tool's JSON Schema. It 500s
/// ("Could not translate instance to regex") when a schema is structurally
/// ambiguous: missing root `type`, untyped properties, or `type: null`. We
/// repair those defects in place and return every tool unchanged otherwise.
/// No tool-count cap and no description truncation — the mcpproxy compact router
/// is what keeps the *set* small; the provider must never drop tools.
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

/// Repair a single JSON Schema object so Outlines can compile it.
fn normalize_schema(mut schema: serde_json::Value) -> serde_json::Value {
    if !schema.is_object() {
        return serde_json::json!({ "type": "object" });
    }

    // Ensure an explicit root type. Outlines needs an explicit object root;
    // set it unconditionally when the schema is an object (whether or not it
    // already declares properties) so the grammar compiler never sees an
    // ambiguous root.
    let root_type = schema.get("type").cloned();
    let is_explicit_object = matches!(root_type, Some(serde_json::Value::String(s)) if s == "object");
    if !is_explicit_object {
        schema["type"] = serde_json::json!("object");
    }

    // Recurse into properties, giving each an explicit type when missing.
    if let Some(props) = schema.get_mut("properties").and_then(|p| p.as_object_mut()) {
        for (_name, prop) in props.iter_mut() {
            if prop.is_object() {
                let t = prop.get("type").cloned();
                let has_type = match &t {
                    Some(serde_json::Value::String(s)) => !s.is_empty() && s != "null",
                    Some(serde_json::Value::Array(a)) => {
                        // oneOf/anyOf-style: keep, but drop bare "null" entries
                        // that Outlines cannot translate.
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
                // Recurse for nested objects.
                if matches!(prop.get("type"), Some(serde_json::Value::String(s)) if s == "object") {
                    *prop = normalize_schema(prop.clone());
                }
            }
        }
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inkling_supports_thinking_with_max_default() {
        let effort_levels = supported_thinking_effort_levels(&nvidia::Model::Inkling);
        let values = effort_levels
            .iter()
            .map(|level| level.value.as_ref())
            .collect::<Vec<_>>();

        assert_eq!(values, ["low", "medium", "high", "xhigh", "max"]);
        assert_eq!(
            effort_levels
                .iter()
                .find(|level| level.is_default)
                .map(|level| level.value.as_ref()),
            Some("max")
        );
    }

    #[test]
    fn normalize_tool_schemas_repairs_structural_defects_without_dropping_tools() {
        let tools = vec![
            // Untyped root + untyped property -> should become object/string.
            language_model::LanguageModelRequestTool {
                name: "a".into(),
                description: "x".repeat(5000),
                input: language_model::LanguageModelRequestToolInput::Function {
                    use_input_streaming: false,
                    input_schema: serde_json::json!({"properties": {"q": {}}}),
                },
            },
            // Nullable property -> bare "null" dropped, keeps "string".
            language_model::LanguageModelRequestTool {
                name: "b".into(),
                description: "keep me".into(),
                input: language_model::LanguageModelRequestToolInput::Function {
                    use_input_streaming: false,
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {"q": {"type": ["string", "null"]}}
                    }),
                },
            },
        ];

        let normalized = normalize_tool_schemas(tools);
        // No tool dropped.
        assert_eq!(normalized.len(), 2);
        // Description preserved verbatim (no truncation).
        assert_eq!(normalized[0].description.chars().count(), 5000);

        let a = match &normalized[0].input {
            language_model::LanguageModelRequestToolInput::Function { input_schema, .. } => input_schema,
            _ => panic!("expected function input"),
        };
        assert_eq!(a["type"], "object");
        assert_eq!(a["properties"]["q"]["type"], "string");

        let b = match &normalized[1].input {
            language_model::LanguageModelRequestToolInput::Function { input_schema, .. } => input_schema,
            _ => panic!("expected function input"),
        };
        assert_eq!(b["properties"]["q"]["type"], serde_json::json!(["string"]));
    }
}
