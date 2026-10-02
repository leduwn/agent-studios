use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// OpenAI Chat Completions request wire format (`POST /v1/chat/completions`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ChatTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ChatToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ChatResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<ChatStreamOptions>,
}

/// Message payload inside a Chat Completions request or response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<ChatMessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Chat message content: either plain string or structured content parts (multimodal).
#[derive(Debug, Clone, PartialEq)]
pub enum ChatMessageContent {
    Text(String),
    Parts(Vec<ChatContentPart>),
}

impl Serialize for ChatMessageContent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Text(text) => serializer.serialize_str(text),
            Self::Parts(parts) => parts.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ChatMessageContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Text(String),
            Parts(Vec<ChatContentPart>),
        }

        match Helper::deserialize(deserializer)? {
            Helper::Text(t) => Ok(ChatMessageContent::Text(t)),
            Helper::Parts(p) => Ok(ChatMessageContent::Parts(p)),
        }
    }
}

/// A multimodal content part in a chat message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatContentPart {
    Text { text: String },
    ImageUrl { image_url: ChatImageUrl },
}

/// URL details for an image content part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Tool definition in Chat Completions wire format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatTool {
    pub r#type: String,
    pub function: ChatFunctionDefinition,
}

/// Function definition nested inside a ChatTool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatFunctionDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Tool choice parameter for Chat Completions.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatToolChoice {
    Auto,
    None,
    Required,
    Function(String),
}

impl Serialize for ChatToolChoice {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Auto => serializer.serialize_str("auto"),
            Self::None => serializer.serialize_str("none"),
            Self::Required => serializer.serialize_str("required"),
            Self::Function(name) => {
                #[derive(Serialize)]
                struct FuncObj<'a> {
                    name: &'a str,
                }
                #[derive(Serialize)]
                struct ToolObj<'a> {
                    r#type: &'static str,
                    function: FuncObj<'a>,
                }
                ToolObj {
                    r#type: "function",
                    function: FuncObj { name },
                }
                .serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for ChatToolChoice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            String(String),
            Object {
                #[allow(dead_code)]
                r#type: String,
                function: FuncHelper,
            },
        }
        #[derive(Deserialize)]
        struct FuncHelper {
            name: String,
        }

        match Helper::deserialize(deserializer)? {
            Helper::String(s) => match s.as_str() {
                "auto" => Ok(ChatToolChoice::Auto),
                "none" => Ok(ChatToolChoice::None),
                "required" => Ok(ChatToolChoice::Required),
                other => Ok(ChatToolChoice::Function(other.to_string())),
            },
            Helper::Object { function, .. } => Ok(ChatToolChoice::Function(function.name)),
        }
    }
}

/// A tool call invocation generated by the assistant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatToolCall {
    pub id: String,
    pub r#type: String,
    pub function: ChatFunctionCall,
}

/// The specific function and serialized arguments of a tool call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatFunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Structured outputs `response_format` configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponseFormat {
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<ChatJsonSchemaFormat>,
}

/// JSON Schema configuration when response_format is "json_schema".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatJsonSchemaFormat {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Stream options for Chat Completions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatStreamOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_usage: Option<bool>,
}

/// A chunk received from an SSE stream of Chat Completions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    #[serde(default)]
    pub object: Option<String>,
    #[serde(default)]
    pub created: Option<i64>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub choices: Vec<ChatChunkChoice>,
    #[serde(default)]
    pub usage: Option<ChatUsage>,
}

/// Choice entry within a stream chunk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatChunkChoice {
    pub index: u32,
    #[serde(default)]
    pub delta: ChatChunkDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// Delta payload inside a stream choice.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChatChunkDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ChatChunkToolCall>>,
}

/// Fragmented tool call inside a stream chunk delta.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatChunkToolCall {
    pub index: usize,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub function: Option<ChatChunkFunctionCall>,
}

/// Fragmented function call arguments inside a stream tool call delta.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChatChunkFunctionCall {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}

/// Usage statistics reported at the end of a Chat Completion stream or response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    #[serde(default)]
    pub prompt_tokens_details: Option<ChatPromptTokensDetails>,
    #[serde(default)]
    pub completion_tokens_details: Option<ChatCompletionTokensDetails>,
}

/// Detailed breakdown of prompt tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatPromptTokensDetails {
    #[serde(default)]
    pub cached_tokens: Option<i64>,
}

/// Detailed breakdown of completion tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionTokensDetails {
    #[serde(default)]
    pub reasoning_tokens: Option<i64>,
}
