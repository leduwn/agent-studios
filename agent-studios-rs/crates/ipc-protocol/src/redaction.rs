use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt;

pub struct SecretRedactor {
    regexes: Vec<Regex>,
}

impl SecretRedactor {
    pub fn new() -> Self {
        let patterns = [
            // Anthropic API keys (e.g. sk-ant-api03-..., sk-ant-admin...)
            r"sk-ant-[a-zA-Z0-9_\-]{16,}",
            // OpenAI API keys (e.g. sk-..., sk-proj-...)
            r"sk-(?:proj-)?[a-zA-Z0-9_\-]{20,}",
            // GitHub personal access & fine-grained tokens (ghp_, gho_, ghu_, ghs_, ghr_)
            r"gh[pousr]_[a-zA-Z0-9_]{16,}",
            // AWS Access Key ID
            r"AKIA[0-9A-Z]{16}",
            // Generic Bearer tokens
            r"(?i)bearer\s+[a-zA-Z0-9_\-\.=+/]{16,}",
            // PEM Private Keys (RSA, EC, DSA, OpenSSH, generic)
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]+?-----END [A-Z ]*PRIVATE KEY-----",
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
                        || k_lower.contains("privatekey")
                        || k_lower.contains("credential")
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

/// A wrapper for sensitive values that enforces safe handling and prevents accidental leakage.
///
/// # Security Contract
/// - **Formatting (`Debug` and `Display`)**: Both implementations always mask the inner
///   value to `"[REDACTED]"`. Even with `{:#?}` or `{}`, secrets will never appear in standard
///   logging, error formatting, or diagnostics.
/// - **Serialization (`Serialize`)**: Note that `serde::Serialize` is implemented transparently
///   to serialize the raw inner `T` for authenticated wire protocol transport across IPC boundaries.
///   Do **NOT** use default serde serialization of `Redacted<T>` for untrusted telemetry, logging,
///   or client-facing analytics.
/// - **Safe Telemetry**: Use [`Redacted::to_masked_value`] or [`MaskedTelemetry`] to obtain a safe,
///   masked JSON representation that is guaranteed never to expose secrets.
/// - **Explicit Exposure**: Access to the underlying raw value is intentionally explicit via
///   [`Redacted::expose_secret`] or [`Redacted::into_inner`].
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

    /// Returns a masked JSON value (`"[REDACTED]"`) safe for telemetry and logging.
    pub fn to_masked_value(&self) -> serde_json::Value {
        serde_json::Value::String("[REDACTED]".to_string())
    }

    /// Converts this redacted wrapper into a `MaskedTelemetry` token that serializes as `"[REDACTED]"`.
    pub fn to_masked_telemetry(&self) -> MaskedTelemetry {
        MaskedTelemetry::new()
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

/// Safe wrapper for telemetry serialization that never exposes the underlying secret.
/// Always serializes, displays, and debugs as `"[REDACTED]"`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MaskedTelemetry(String);

impl MaskedTelemetry {
    pub fn new() -> Self {
        Self("[REDACTED]".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for MaskedTelemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MaskedTelemetry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl fmt::Debug for MaskedTelemetry {
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

    #[test]
    fn test_redact_openai_key() {
        let redactor = SecretRedactor::new();
        let standard_key = "OpenAI key sk-abcdef1234567890abcdef1234567890 in config";
        let proj_key = "OpenAI project key sk-proj-abcdef1234567890abcdef1234567890 in env";

        assert_eq!(
            redactor.redact_text(standard_key),
            "OpenAI key [REDACTED] in config"
        );
        assert_eq!(
            redactor.redact_text(proj_key),
            "OpenAI project key [REDACTED] in env"
        );
    }

    #[test]
    fn test_redact_github_tokens() {
        let redactor = SecretRedactor::new();
        let ghp = "export GITHUB_TOKEN=ghp_abcdef1234567890abcdef1234567890";
        let gho = "OAuth token: gho_abcdef1234567890abcdef1234567890";
        let ghs = "Server token: ghs_abcdef1234567890abcdef1234567890";

        assert_eq!(redactor.redact_text(ghp), "export GITHUB_TOKEN=[REDACTED]");
        assert_eq!(redactor.redact_text(gho), "OAuth token: [REDACTED]");
        assert_eq!(redactor.redact_text(ghs), "Server token: [REDACTED]");
    }

    #[test]
    fn test_redact_aws_key() {
        let redactor = SecretRedactor::new();
        let aws = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE";
        assert_eq!(redactor.redact_text(aws), "AWS_ACCESS_KEY_ID=[REDACTED]");
    }

    #[test]
    fn test_redact_pem_private_keys() {
        let redactor = SecretRedactor::new();

        let rsa_key = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA0Y1234567890abcdef\n-----END RSA PRIVATE KEY-----";
        let generic_key = "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBgkqhkiG9w0BAQEFAASC\n-----END PRIVATE KEY-----";

        assert_eq!(redactor.redact_text(rsa_key), "[REDACTED]");
        assert_eq!(redactor.redact_text(generic_key), "[REDACTED]");
    }

    #[test]
    fn test_redact_deep_nested_json_depth_5_plus() {
        let redactor = SecretRedactor::new();
        let deep_json = serde_json::json!({
            "level1": {
                "level2": {
                    "level3": {
                        "level4": {
                            "level5": {
                                "level6": {
                                    "secret_token": "my-ultra-secret",
                                    "user_credential": "super-credential",
                                    "embedded_key": "Use sk-ant-api03-abcdef1234567890abcdef1234567890 for API",
                                    "innocent_string": "safe data at depth 6",
                                    "number_value": 42
                                }
                            }
                        }
                    }
                }
            }
        });

        let redacted = redactor.redact_json(&deep_json);
        let l6 = &redacted["level1"]["level2"]["level3"]["level4"]["level5"]["level6"];
        assert_eq!(l6["secret_token"], "[REDACTED]");
        assert_eq!(l6["user_credential"], "[REDACTED]");
        assert_eq!(l6["embedded_key"], "Use [REDACTED] for API");
        assert_eq!(l6["innocent_string"], "safe data at depth 6");
        assert_eq!(l6["number_value"], 42);
    }

    #[test]
    fn test_redact_arrays_of_objects_and_mixed_keys() {
        let redactor = SecretRedactor::new();
        let payload = serde_json::json!({
            "items": [
                {
                    "id": 1,
                    "API_KEY": "raw-key-1",
                    "comment": "normal comment"
                },
                {
                    "id": 2,
                    "metadata": {
                        "PRIVATE_KEY": "pem-data",
                        "authHeader": "Bearer 1234567890abcdef1234567890"
                    }
                },
                {
                    "id": 3,
                    "tokens": "plain_token_string",
                    "tags": [
                        "AKIAIOSFODNN7EXAMPLE",
                        "ghp_abcdef1234567890abcdef1234567890"
                    ]
                }
            ]
        });

        let redacted = redactor.redact_json(&payload);
        let items = redacted["items"].as_array().unwrap();

        // Object 1: API_KEY redacted
        assert_eq!(items[0]["API_KEY"], "[REDACTED]");
        assert_eq!(items[0]["comment"], "normal comment");

        // Object 2: PRIVATE_KEY redacted, Bearer token redacted in authHeader (key matches authorization / token)
        assert_eq!(items[1]["metadata"]["PRIVATE_KEY"], "[REDACTED]");
        assert_eq!(items[1]["metadata"]["authHeader"], "[REDACTED]");

        // Object 3: sensitive key "tokens" redacted, and array elements with secrets inside non-sensitive "tags" redacted
        assert_eq!(items[2]["tokens"], "[REDACTED]");
        assert_eq!(items[2]["tags"][0], "[REDACTED]");
        assert_eq!(items[2]["tags"][1], "[REDACTED]");
    }

    #[test]
    fn test_redacted_security_contract_and_safe_telemetry() {
        let secret_value = "sk-ant-api03-abcdef1234567890abcdef1234567890".to_string();
        let redacted = Redacted::new(secret_value.clone());

        // 1. Debug and Display never leak raw secret
        assert_eq!(format!("{redacted}"), "[REDACTED]");
        assert_eq!(format!("{redacted:?}"), "[REDACTED]");
        assert_eq!(format!("{redacted:#?}"), "[REDACTED]");

        // 2. Raw serialization for wire transport
        let wire_json = serde_json::to_string(&redacted).expect("Serialize wire format");
        assert_eq!(wire_json, format!("\"{secret_value}\""));

        // 3. Safe telemetry serialization never exposes secret
        let masked_val = redacted.to_masked_value();
        assert_eq!(
            masked_val,
            serde_json::Value::String("[REDACTED]".to_string())
        );

        let masked_telem = redacted.to_masked_telemetry();
        let telem_json = serde_json::to_string(&masked_telem).expect("Serialize telemetry");
        assert_eq!(telem_json, "\"[REDACTED]\"");
        assert_eq!(format!("{masked_telem}"), "[REDACTED]");
        assert_eq!(format!("{masked_telem:?}"), "[REDACTED]");
    }
}
