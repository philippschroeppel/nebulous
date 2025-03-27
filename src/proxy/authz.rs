use crate::models::{V1AuthzConfig, V1UserProfile};
use serde_json::Value;
use std::collections::HashMap;
use tracing::debug;

/// Expand known placeholders in patterns such as `${email}`, `${org_id}`, etc.
pub fn expand_pattern(
    raw_pattern: &str,
    user: &V1UserProfile,
    org_id: &str,
    org_info: &HashMap<String, String>,
) -> String {
    let mut result = raw_pattern.to_string();
    // Substitute placeholders
    result = result.replace("${email}", &user.email);
    result = result.replace("${org_id}", org_id);

    if let Some(org_name) = org_info.get("org_name") {
        result = result.replace("${org_name}", org_name);
    }
    if let Some(org_role) = org_info.get("org_role") {
        result = result.replace("${org_role}", org_role);
    }
    if let Some(handle) = &user.handle {
        result = result.replace("${handle}", handle);
    }
    result
}

/// Basic path/wildcard matching function:
pub fn path_matches(pattern: &str, actual_path: &str) -> bool {
    // Interpret "/somepath/**" as "/somepath/...anything..."
    if let Some(stripped) = pattern.strip_suffix("/**") {
        actual_path.starts_with(stripped)
    } else {
        // If no "**" suffix, do exact match
        pattern == actual_path
    }
}

/// Checks if a given JSON body field matches a pattern:
pub fn field_matches(json_body: &Value, field: &str, pattern: &str) -> bool {
    if let Some(val) = json_body.get(field).and_then(|v| v.as_str()) {
        // If pattern ends with "/**", check prefix; otherwise exact match
        if let Some(stripped) = pattern.strip_suffix("/**") {
            return val.starts_with(stripped);
        } else {
            return val == pattern;
        }
    }
    false
}

/// Evaluate the list of authorization rules against the request path and (optional) JSON body.
/// This function sets `is_allowed` to true/false depending on the matched rules.
pub fn evaluate_authorization_rules(
    is_allowed: &mut bool,
    user_profile: &V1UserProfile,
    authz_config: &V1AuthzConfig,
    request_path: &str,
    json_body_opt: Option<&Value>,
) {
    // If no rules, nothing to do:
    let Some(rules) = &authz_config.rules else {
        return;
    };

    'outer: for rule in rules {
        let orgs_map = user_profile
            .organizations
            .as_ref()
            .map(Clone::clone)
            .unwrap_or_default();

        debug!("[PROXY] Org map: {orgs_map:?}");

        // We'll consider a rule "matched" if *any* expansions match for any org:
        let mut rule_matches = false;

        // For each org the user is in, expand placeholders and check:
        for (org_id, org_info) in orgs_map.iter() {
            debug!("[PROXY] Org ID: {org_id}, Org info: {org_info:?}");

            // If there's a path_match section in the rule, check it:
            if let Some(path_match_cfg) = &rule.path_match {
                for pm in path_match_cfg {
                    let expanded = expand_pattern(
                        pm.pattern.as_deref().unwrap_or(""),
                        user_profile,
                        org_id,
                        org_info,
                    );
                    if path_matches(&expanded, request_path) {
                        rule_matches = true;
                        break;
                    }
                }
            }

            debug!("[PROXY] path rule matches: {rule_matches}");

            // If the rule didn't match on path, optionally check field matches:
            if !rule_matches {
                if let Some(field_match_cfg) = &rule.field_match {
                    if let Some(json_body) = json_body_opt {
                        for fm in field_match_cfg {
                            let expanded = expand_pattern(
                                fm.pattern.as_deref().unwrap_or(""),
                                user_profile,
                                org_id,
                                org_info,
                            );
                            if field_matches(
                                json_body,
                                fm.json_path.as_deref().unwrap_or(""),
                                &expanded,
                            ) {
                                rule_matches = true;
                            }
                        }
                    }
                }
            }

            debug!("[PROXY] field rule matches: {rule_matches}");

            // If we found a match for this org, apply rule and break out:
            if rule_matches {
                *is_allowed = rule.allow;
                // If the rule is allow, short-circuit. If it's deny, short-circuit as well.
                // The logic here might differ if you want to continue checking rules after a deny.
                break 'outer;
            }
        }
    }
}

pub fn extract_json_path(json_obj: &Value, json_path: &str) -> Option<Value> {
    // 1) Compile the JSONPath expression
    let compiled = jsonpath_lib::Compiled::compile(json_path).ok()?;
    // 2) Run it on your json_obj
    let results = compiled.select(json_obj).ok()?;
    // 3) Return the first match, if any. For multiple matches, adapt as needed.
    if let Some(value) = results.get(0) {
        Some((*value).clone())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        V1AuthzConfig, V1AuthzFieldMatch, V1AuthzPathMatch, V1AuthzRule, V1UserProfile,
    };
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn test_expand_pattern() {
        // Arrange
        let user_profile = V1UserProfile {
            email: String::from("john.doe@example.com"),
            handle: Some(String::from("john_handle")),
            organizations: None,
            ..Default::default()
        };
        let org_id = "org-123";
        let mut org_info = HashMap::new();
        org_info.insert("org_name".to_string(), "Example Org".to_string());
        org_info.insert("org_role".to_string(), "Admin".to_string());

        let raw_pattern = "User: ${email}, OrgID: ${org_id}, OrgName: ${org_name}, Role: ${org_role}, Handle: ${handle}";

        // Act
        let result = expand_pattern(raw_pattern, &user_profile, org_id, &org_info);

        // Assert
        assert_eq!(
            result,
            "User: john.doe@example.com, OrgID: org-123, OrgName: Example Org, Role: Admin, Handle: john_handle"
        );
    }

    #[test]
    fn test_path_matches() {
        // Exact match
        assert!(path_matches("/api/v1/resource", "/api/v1/resource"));
        assert!(!path_matches("/api/v1/resource", "/api/v1/non-matching"));

        // Wildcard suffix "/**"
        assert!(path_matches("/api/v1/resource/**", "/api/v1/resource"));
        assert!(path_matches(
            "/api/v1/resource/**",
            "/api/v1/resource/subpath"
        ));
        assert!(path_matches("/api/v1/**", "/api/v1/resource/subpath"));
        assert!(!path_matches("/api/v2/**", "/api/v1/resource"));
    }

    #[test]
    fn test_field_matches() {
        // Arrange
        let json_body = json!({
            "action": "create",
            "resource": "repo/subpath",
            "notes": "some notes"
        });

        // Act & Assert
        // Exact match
        assert!(field_matches(&json_body, "action", "create"));
        assert!(!field_matches(&json_body, "action", "delete"));

        // Prefix match via "/**"
        assert!(field_matches(&json_body, "resource", "repo/**"));
        assert!(!field_matches(&json_body, "resource", "other/**"));
    }

    #[test]
    fn test_evaluate_authorization_rules_path_match_allows() {
        // Arrange
        let mut is_allowed = false;
        let user_profile = V1UserProfile {
            email: String::from("alice@example.com"),
            handle: None,
            organizations: Some({
                let mut map = HashMap::new();
                map.insert("org-abc".to_string(), {
                    let mut org_info = HashMap::new();
                    org_info.insert("org_name".to_string(), "TestOrg".to_string());
                    org_info
                });
                map
            }),
            ..Default::default()
        };

        // Define rules
        let rules = vec![V1AuthzRule {
            name: "test".to_string(),
            allow: true, // If matched, this rule allows
            path_match: Some(vec![V1AuthzPathMatch {
                pattern: Some("/api/v1/**".to_string()),
                path: None,
            }]),
            field_match: None,
            rule_match: None,
        }];

        let authz_config = V1AuthzConfig {
            enabled: true,
            default_action: "allow".to_string(),
            auth_type: "jwt".to_string(),
            jwt: None,
            rules: Some(rules),
        };

        // Act
        evaluate_authorization_rules(
            &mut is_allowed,
            &user_profile,
            &authz_config,
            "/api/v1/some-resource",
            None,
        );

        // Assert
        assert!(is_allowed, "Should be allowed by matching path rule.");
    }

    #[test]
    fn test_evaluate_authorization_rules_field_match_denies() {
        // Arrange
        let mut is_allowed = true; // starting as true, test that rule flips to deny
        let user_profile = V1UserProfile {
            email: String::from("bob@example.com"),
            handle: Some("bob_handle".to_string()),
            organizations: Some({
                let mut map = HashMap::new();
                map.insert("org-lmn".to_string(), {
                    let mut org_info = HashMap::new();
                    org_info.insert("org_role".to_string(), "Viewer".to_string());
                    org_info
                });
                map
            }),
            ..Default::default()
        };

        // Define rules
        let rules = vec![
            // This rule will deny if field matches "action=delete"
            V1AuthzRule {
                name: "test1".to_string(),
                allow: false,
                rule_match: None,
                path_match: None,
                field_match: Some(vec![V1AuthzFieldMatch {
                    json_path: Some("action".to_string()),
                    pattern: Some("delete".to_string()),
                }]),
            },
        ];

        let authz_config = V1AuthzConfig {
            enabled: true,
            default_action: "allow".to_string(),
            auth_type: "jwt".to_string(),
            jwt: None,
            rules: Some(rules),
        };

        let request_path = "/api/other"; // won't matter
        let json_body = json!({
            "action": "delete",
        });

        // Act
        evaluate_authorization_rules(
            &mut is_allowed,
            &user_profile,
            &authz_config,
            request_path,
            Some(&json_body),
        );

        // Assert
        assert!(!is_allowed, "Rule should deny due to field match.");
    }

    #[test]
    fn test_extract_json_path() {
        // Arrange
        let json_obj = json!({
            "parent": {
                "child": {
                    "value": 123,
                    "list": [10, 20, 30]
                }
            }
        });

        // Act
        let found_value = extract_json_path(&json_obj, "$.parent.child.value");
        let found_list_item = extract_json_path(&json_obj, "$.parent.child.list[1]");
        let not_found = extract_json_path(&json_obj, "$.parent.nonexistent");

        // Assert
        assert_eq!(found_value, Some(json!(123)));
        assert_eq!(found_list_item, Some(json!(20)));
        assert_eq!(not_found, None);
    }
}
