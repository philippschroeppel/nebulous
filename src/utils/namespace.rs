use crate::models::V1UserProfile;

pub fn resolve_namespace(namespace: &str, user_profile: &V1UserProfile) -> String {
    if namespace == "-" {
        user_profile.handle.clone().unwrap_or(
            user_profile
                .email
                .clone()
                .replace("@", "-")
                .replace(".", "-"),
        )
    } else {
        namespace.to_string()
    }
}
