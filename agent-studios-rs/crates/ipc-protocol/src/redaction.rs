use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt;

pub struct SecretRedactor {
    regexes: Vec<Regex>,
}

impl SecretRedactor {
    pub fn new() -> Self {
        let patterns = [
            // Anthropic API keys
            r"sk-ant-[a-zA-Z0-9_\-]{16,}",
            // OpenAI API keys
            r"sk-[a-zA-Z0-9]{20,}",
            // GitHub tokens
            r"gh[pousr]_[a-zA-Z0-9]{30,}",
            // AWS Access Key ID
            r"AKIA[0-9A-Z]{16}",
            // Generic Bearer tokens
            r"(?i)bearer\s+[a-zA-Z0-9_\-\.]{16,}",
            // Private Keys
            r"-----BEGIN [A-Z ]+PRIVATE KEY-----[\s\S]+?-----END [A-Z ]+PRIVATE KEY-----",
        ];

        let regexes = patterns
            .iter()
            .map(|p| Regex::new(p).expect("Valid regex pattern"))
            .collect();

        Self { regexes }
    }

    pub fn redact_text(&self, text: &str) -> String {
        let mut result = text.to_string();
        for re in &self.regexes {
            result = re.replace_all(&result, "[REDACTED]").to_string();
        }
        result
    }

    pub fn redact_json(&self, value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => serde_json::Value::String(self.redact_text(s)),
            serde_json::Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(|v| self.redact_json(v)).collect())
            }
            serde_json::Value::Object(map) => {
                let mut redacted_map = serde_json::Map::new();
                for (k, v) in map {
                    let k_lower = k.to_lowercase();
                    if k_lower.contains("secret")
                        || k_lower.contains("password")
                        || k_lower.contains("token")
                        || k_lower.contains("api_key")
                        || k_lower.contains("apikey")
                        || k_lower.contains("authorization")
                        || k_lower.contains("private_key")
                    {
                        redacted_map.insert(
                            k.clone(),
                            serde_json::Value::String("[REDACTED]".to_string()),
                        );
                    } else {
                        redacted_map.insert(k.clone(), self.redact_json(v));
                    }
                }
                serde_json::Value::Object(redacted_map)
            }
            other => other.clone(),
        }
    }
}

impl Default for SecretRedactor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Redacted<T>(T);

impl<T> Redacted<T> {
    pub fn new(inner: T) -> Self {
        Self(inner)
    }

    pub fn expose_secret(&self) -> &T {
        &self.0
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> fmt::Debug for Redacted<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl<T> fmt::Display for Redacted<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[REDACTED]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_anthropic_key() {
        let redactor = SecretRedactor::new();
        let input =
            "Calling Claude with key sk-ant-api03-abcdef1234567890abcdef1234567890 for prompt";
        let output = redactor.redact_text(input);
        assert!(!output.contains("sk-ant-api03"));
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn test_redact_bearer_token() {
        let redactor = SecretRedactor::new();
        let input = "Authorization: Bearer 1234567890abcdef1234567890";
        let output = redactor.redact_text(input);
        assert_eq!(output, "Authorization: [REDACTED]");
    }

    #[test]
    fn test_redact_json_sensitive_keys() {
        let redactor = SecretRedactor::new();
        let json = serde_json::json!({
            "user": "alice",
            "api_key": "some_secret_value",
            "metadata": {
                "password": "supersecretpassword",
                "normal": "safe_data"
            }
        });

        let redacted = redactor.redact_json(&json);
        assert_eq!(redacted["user"], "alice");
        assert_eq!(redacted["api_key"], "[REDACTED]");
        assert_eq!(redacted["metadata"]["password"], "[REDACTED]");
        assert_eq!(redacted["metadata"]["normal"], "safe_data");
    }

    #[test]
    fn test_redacted_type_debug_display() {
        let secret = Redacted::new("super-secret-token".to_string());
        assert_eq!(format!("{secret}"), "[REDACTED]");
        assert_eq!(format!("{secret:?}"), "[REDACTED]");
        assert_eq!(secret.expose_secret(), "super-secret-token");
    }
}
