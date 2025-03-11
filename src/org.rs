use std::collections::HashMap;

pub fn get_organization_names(
    organizations: &HashMap<String, HashMap<String, String>>,
) -> Vec<String> {
    organizations
        .values()
        .filter_map(|org_info| org_info.get("org_name").cloned())
        .collect()
}
