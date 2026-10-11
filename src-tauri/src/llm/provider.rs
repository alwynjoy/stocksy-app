//! Provider trait + common types

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value, // parsed JSON object
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: ChatUsage,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChatOptions {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub api_key: String,
    pub system: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDef>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub thinking_budget: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user", "assistant", "tool"
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value, // JSON schema
}

#[derive(Debug, Clone)]
pub struct TokenEvent {
    pub delta: String,
    pub provider: String,
    pub model: String,
}
