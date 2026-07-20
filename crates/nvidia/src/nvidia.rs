use anyhow::Result;
use serde::{Deserialize, Serialize};
use strum::EnumIter;

/// NVIDIA NIM (Inkling) OpenAI-compatible chat completions base URL.
pub const NVIDIA_API_URL: &str = "https://integrate.api.nvidia.com/v1";

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, EnumIter)]
pub enum Model {
    #[default]
    #[serde(rename = "thinkingmachines/inkling")]
    Inkling,
    #[serde(rename = "custom")]
    Custom {
        name: String,
        /// The name displayed in the UI, such as in the agent panel model dropdown menu.
        display_name: Option<String>,
        max_tokens: u64,
        max_output_tokens: Option<u64>,
        max_completion_tokens: Option<u64>,
        supports_images: Option<bool>,
        supports_tools: Option<bool>,
        parallel_tool_calls: Option<bool>,
    },
}

impl Model {
    pub fn default_fast() -> Self {
        Self::Inkling
    }

    pub fn from_id(id: &str) -> Result<Self> {
        match id {
            "thinkingmachines/inkling" => Ok(Self::Inkling),
            _ => anyhow::bail!("invalid model id '{id}'"),
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Inkling => "thinkingmachines/inkling",
            Self::Custom { name, .. } => name,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Inkling => "NVIDIA NIM Inkling",
            Self::Custom {
                name, display_name, ..
            } => display_name.as_ref().unwrap_or(name),
        }
    }

    pub fn max_token_count(&self) -> u64 {
        match self {
            Self::Inkling => 1_048_576,
            Self::Custom { max_tokens, .. } => *max_tokens,
        }
    }

    pub fn max_output_tokens(&self) -> Option<u64> {
        match self {
            Self::Inkling => Some(16384),
            Self::Custom {
                max_output_tokens, ..
            } => *max_output_tokens,
        }
    }

    pub fn supports_parallel_tool_calls(&self) -> bool {
        match self {
            Self::Inkling => true,
            Self::Custom {
                parallel_tool_calls: Some(support),
                ..
            } => *support,
            Model::Custom { .. } => false,
        }
    }

    pub fn requires_json_schema_subset(&self) -> bool {
        // NVIDIA NIM's Outlines-based tool parser needs the FULL JSON Schema,
        // not the OpenAI subset. Returning false selects JsonSchema format.
        false
    }

    pub fn supports_prompt_cache_key(&self) -> bool {
        false
    }

    pub fn supports_tool(&self) -> bool {
        match self {
            Self::Inkling => true,
            Self::Custom {
                supports_tools: Some(support),
                ..
            } => *support,
            Model::Custom { .. } => false,
        }
    }

    pub fn supports_images(&self) -> bool {
        match self {
            Self::Inkling => false,
            Self::Custom {
                supports_images: Some(support),
                ..
            } => *support,
            Self::Custom { .. } => false,
        }
    }

    pub fn supports_reasoning_effort(&self) -> bool {
        match self {
            Self::Inkling => true,
            Self::Custom { .. } => false,
        }
    }
}
